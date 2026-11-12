//! Builds synthetic `.dat` archives so tests do not depend on the game data.

use std::io::Write;
use std::path::Path;

use flate2::Compression;
use flate2::write::ZlibEncoder;

struct Entry {
    path: String,
    flags: u32,
    stored: Vec<u8>,
    declared_size: u32,
}

/// Writes archives in the layout `ArcanumArchive` reads: data, index, 28-byte footer.
#[derive(Default)]
pub struct DatBuilder {
    entries: Vec<Entry>,
}

impl DatBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// An uncompressed file (flag `0x1`).
    pub fn raw(self, path: &str, data: &[u8]) -> Self {
        let size = data.len() as u32;
        self.entry(path, 0x1, data.to_vec(), size)
    }

    /// A zlib-compressed file (flag `0x2`).
    pub fn zlib(self, path: &str, data: &[u8]) -> Self {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(data).unwrap();
        let size = data.len() as u32;
        self.entry(path, 0x2, enc.finish().unwrap(), size)
    }

    /// A directory entry (flag `0x400`), with no data.
    pub fn dir(self, path: &str) -> Self {
        self.entry(path, 0x400, Vec::new(), 0)
    }

    /// Any entry, for building malformed archives.
    pub fn entry(mut self, path: &str, flags: u32, stored: Vec<u8>, declared_size: u32) -> Self {
        self.entries.push(Entry { path: path.to_string(), flags, stored, declared_size });
        self
    }

    pub fn build(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut offsets = Vec::new();
        for e in &self.entries {
            offsets.push(if e.flags & 0x400 != 0 { 0 } else { out.len() as u32 });
            out.extend_from_slice(&e.stored);
        }
        let index_start = out.len();
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for (e, offset) in self.entries.iter().zip(offsets) {
            out.extend_from_slice(&(e.path.len() as u32 + 1).to_le_bytes());
            out.extend_from_slice(e.path.as_bytes());
            out.push(0);
            for v in [0, e.flags, e.declared_size, e.stored.len() as u32, offset] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        let index_size = (out.len() - index_start + 28) as u32;
        out.extend_from_slice(&[0xAB; 16]); // GUID
        out.extend_from_slice(b"1TAD");
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&index_size.to_le_bytes());
        out
    }

    pub fn write(&self, path: impl AsRef<Path>) {
        std::fs::write(path, self.build()).unwrap();
    }
}
