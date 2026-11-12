use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub struct ArcanumArchive {
    file: File,
    // We will populate file entries here after parsing the TOC
}

impl ArcanumArchive {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut file = File::open(path)?;

        // Troika .dat files typically start with a 4-byte or 8-byte header 
        // indicating the number of files packed inside. Let's read the file count first:
        let mut count_buf = [0u8; 4];
        file.read_exact(&mut count_buf)?;
        let file_count = u32::from_le_bytes(count_buf);

        println!("Successfully opened archive. Total files indexed in TOC: {}", file_count);

        Ok(Self { file })
    }
}