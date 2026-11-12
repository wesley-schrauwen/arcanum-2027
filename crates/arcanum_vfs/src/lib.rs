//! Reader for Troika `.dat` archives (`DAT1`, "1TAD" footer magic).
//!
//! Layout (little endian):
//! - File end: `[index][footer]`, last 4 bytes = size of index + footer.
//! - Footer (28 bytes): GUID[16], "1TAD", u32 (unused), u32 index_size.
//! - Index: u32 count, then per entry:
//!   u32 name_len, name[name_len] (NUL terminated, `\` separators),
//!   u32 unknown, u32 flags, u32 uncompressed_size, u32 compressed_size, u32 offset.

use std::collections::HashMap;
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
fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_ascii_lowercase()
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

        Ok(Self { file, entries, lookup })
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

    /// Read and, if needed, decompress an entry's contents.
    pub fn read_entry(&mut self, entry: &TocEntry) -> io::Result<Vec<u8>> {
        if entry.is_dir() {
            return Err(invalid("entry is a directory"));
        }
        self.file.seek(SeekFrom::Start(entry.offset as u64))?;
        let mut raw = vec![0u8; entry.compressed_size as usize];
        self.file.read_exact(&mut raw)?;

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
    pub fn read(&mut self, path: &str) -> io::Result<Vec<u8>> {
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
        let mut archive = ArcanumArchive::open(path).unwrap();
        assert!(!archive.entries().is_empty());
        let files: Vec<_> = archive.entries().iter().filter(|e| !e.is_dir()).cloned().collect();
        for e in &files {
            archive.read_entry(e).unwrap();
        }
        let first = files[0].path.to_uppercase().replace('\\', "/");
        assert!(archive.find(&first).is_some());
    }

    #[test]
    fn rejects_non_dat() {
        let p = std::env::temp_dir().join("arcanum_vfs_not_a_dat.bin");
        std::fs::write(&p, vec![0u8; 64]).unwrap();
        assert!(ArcanumArchive::open(&p).is_err());
    }
}
