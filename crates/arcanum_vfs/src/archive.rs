//! Reader for Troika `.dat` archives (`DAT1`, "1TAD" footer magic).
//!
//! Layout (little endian):
//! - File end: `[index][footer]`, last 4 bytes = size of index + footer.
//! - Footer (28 bytes): GUID[16], "1TAD", u32 (unused), u32 index_size.
//! - Index: u32 count, then per entry:
//!   u32 name_len, name[name_len] (NUL terminated, `\` separators),
//!   u32 unknown, u32 flags, u32 uncompressed_size, u32 compressed_size, u32 offset.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use flate2::read::ZlibDecoder;

const FOOTER_SIZE: u64 = 28;
const MAGIC: &[u8; 4] = b"1TAD";
const MAX_NAME_LEN: u32 = 4096;

const FLAG_RAW: u32 = 0x1;
const FLAG_ZLIB: u32 = 0x2;
const FLAG_DIR: u32 = 0x400;

#[derive(Debug, Clone)]
pub struct TocEntry {
    /// Path as stored, e.g. `rules\item.mes`.
    pub path: String,
    pub offset: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub flags: u32,
}

impl TocEntry {
    pub fn is_dir(&self) -> bool {
        self.flags & FLAG_DIR != 0
    }

    pub fn is_compressed(&self) -> bool {
        self.flags & FLAG_ZLIB != 0
    }
}

pub struct ArcanumArchive {
    file: File,
    entries: Vec<TocEntry>,
    lookup: HashMap<String, usize>,
    /// Normalised directory path -> (normalised child path -> child path as stored, `/` separated).
    children: HashMap<String, BTreeMap<String, String>>,
}

#[cfg(unix)]
fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, buf, offset)
}

#[cfg(windows)]
fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match file.seek_read(buf, offset)? {
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            n => {
                buf = &mut buf[n..];
                offset += n as u64;
            }
        }
    }
    Ok(())
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

/// Case-insensitive, separator-insensitive key.
pub(crate) fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_ascii_lowercase()
}

fn build_children(entries: &[TocEntry]) -> HashMap<String, BTreeMap<String, String>> {
    let mut children: HashMap<String, BTreeMap<String, String>> = HashMap::new();
    children.entry(String::new()).or_default();
    for e in entries {
        let full = e.path.replace('\\', "/");
        let full = full.trim_matches('/');
        let mut parent_key = String::new();
        let mut end = 0;
        // Register every path prefix as a child of the prefix before it.
        for part in full.split('/') {
            end += part.len();
            let display = &full[..end];
            let key = normalize(display);
            children.entry(parent_key.clone()).or_default().entry(key.clone()).or_insert_with(|| display.to_string());
            if end < full.len() || e.is_dir() {
                children.entry(key.clone()).or_default();
            }
            parent_key = key;
            end += 1; // the '/'
        }
    }
    children
}

impl ArcanumArchive {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let len = file.seek(SeekFrom::End(0))?;
        if len < FOOTER_SIZE {
            return Err(invalid("file too small to be a DAT archive"));
        }

        file.seek(SeekFrom::Start(len - FOOTER_SIZE))?;
        let mut footer = [0u8; FOOTER_SIZE as usize];
        file.read_exact(&mut footer)?;
        if &footer[16..20] != MAGIC {
            return Err(invalid("missing 1TAD footer magic"));
        }
        let index_size = u32::from_le_bytes(footer[24..28].try_into().unwrap()) as u64;
        if index_size < FOOTER_SIZE + 4 || index_size > len {
            return Err(invalid("invalid index size in footer"));
        }

        let index_start = len - index_size;
        file.seek(SeekFrom::Start(index_start))?;
        let mut index = vec![0u8; (index_size - FOOTER_SIZE) as usize];
        file.read_exact(&mut index)?;

        let entries = Self::parse_index(&index, index_start)?;
        let lookup = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (normalize(&e.path), i))
            .collect();

        let children = build_children(&entries);

        Ok(Self { file, entries, lookup, children })
    }

    fn parse_index(buf: &[u8], data_end: u64) -> io::Result<Vec<TocEntry>> {
        let mut r = io::Cursor::new(buf);
        let count = read_u32(&mut r)?;
        // Each entry is at least 24 bytes; reject absurd counts before allocating.
        if (count as u64) * 24 > buf.len() as u64 {
            return Err(invalid("entry count exceeds index size"));
        }
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let name_len = read_u32(&mut r)?;
            if name_len > MAX_NAME_LEN {
                return Err(invalid("entry name too long"));
            }
            let mut name = vec![0u8; name_len as usize];
            r.read_exact(&mut name)?;
            if name.last() == Some(&0) {
                name.pop();
            }
            let path = name.iter().map(|&b| b as char).collect::<String>(); // Latin-1

            let _unknown = read_u32(&mut r)?;
            let flags = read_u32(&mut r)?;
            let uncompressed_size = read_u32(&mut r)?;
            let compressed_size = read_u32(&mut r)?;
            let offset = read_u32(&mut r)?;

            if flags & FLAG_DIR == 0 && offset as u64 + compressed_size as u64 > data_end {
                return Err(invalid(format!("entry {path} extends past data section")));
            }
            entries.push(TocEntry { path, offset, compressed_size, uncompressed_size, flags });
        }
        Ok(entries)
    }

    pub fn entries(&self) -> &[TocEntry] {
        &self.entries
    }

    /// Case-insensitive lookup; accepts `/` or `\` separators.
    pub fn find(&self, path: &str) -> Option<&TocEntry> {
        self.lookup.get(&normalize(path)).map(|&i| &self.entries[i])
    }

    /// Whether `path` names a directory (stored or implied by a file path).
    pub fn is_dir(&self, path: &str) -> bool {
        self.children.contains_key(&normalize(path))
    }

    /// Immediate children of a directory as stored paths (`/` separated), or `None` if
    /// `path` is not a directory. The empty string is the archive root.
    pub fn list_dir(&self, path: &str) -> Option<Vec<&str>> {
        self.children
            .get(&normalize(path))
            .map(|c| c.values().map(String::as_str).collect())
    }

    /// Read and, if needed, decompress an entry's contents.
    pub fn read_entry(&self, entry: &TocEntry) -> io::Result<Vec<u8>> {
        if entry.is_dir() {
            return Err(invalid("entry is a directory"));
        }
        let mut raw = vec![0u8; entry.compressed_size as usize];
        read_exact_at(&self.file, &mut raw, entry.offset as u64)?;

        let data = if entry.is_compressed() {
            let mut out = Vec::with_capacity(entry.uncompressed_size as usize);
            ZlibDecoder::new(raw.as_slice()).read_to_end(&mut out)?;
            out
        } else if entry.flags & FLAG_RAW != 0 || entry.flags == 0 {
            raw
        } else {
            return Err(invalid(format!("unknown entry flags {:#x}", entry.flags)));
        };

        if data.len() != entry.uncompressed_size as usize {
            return Err(invalid(format!("size mismatch for {}", entry.path)));
        }
        Ok(data)
    }

    /// Read a file by path.
    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let entry = self
            .find(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_string()))?;
        self.read_entry(&entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Arcanum/arcanum1.dat");

    #[test]
    fn reads_every_file() {
        let path = Path::new(DAT);
        if !path.exists() {
            eprintln!("skipping: {DAT} not found");
            return;
        }
        let archive = ArcanumArchive::open(path).unwrap();
        assert!(!archive.entries().is_empty());
        let files: Vec<_> = archive.entries().iter().filter(|e| !e.is_dir()).cloned().collect();
        for e in &files {
            archive.read_entry(e).unwrap();
        }
        let first = files[0].path.to_uppercase().replace('\\', "/");
        assert!(archive.find(&first).is_some());
    }

    #[test]
    fn lists_directories() {
        let path = Path::new(DAT);
        if !path.exists() {
            return;
        }
        let archive = ArcanumArchive::open(path).unwrap();
        assert!(archive.is_dir("ART/Interface"));
        assert!(!archive.is_dir("art/interface/mainmenuback.art"));
        let root = archive.list_dir("").unwrap();
        assert!(root.iter().any(|p| p.eq_ignore_ascii_case("art")));
        let iface = archive.list_dir("art\\interface").unwrap();
        assert!(iface.iter().any(|p| p.eq_ignore_ascii_case("art/interface/mainmenuback.art")));
        assert!(archive.list_dir("nope").is_none());
    }

    #[test]
    fn rejects_non_dat() {
        let p = std::env::temp_dir().join("arcanum_vfs_not_a_dat.bin");
        std::fs::write(&p, vec![0u8; 64]).unwrap();
        assert!(ArcanumArchive::open(&p).is_err());
    }
}
