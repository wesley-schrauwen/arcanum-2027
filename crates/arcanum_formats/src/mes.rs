//! Decoder for Arcanum `.mes` message files.
//!
//! A `.mes` file is a sequence of `{id}{text}` pairs. Everything outside
//! braces is ignored, including `//` comments and any text trailing an entry.
//! Text may span multiple lines. Bytes are Windows-1252; line endings in the
//! decoded text are normalised to `\n`. If an entry has more than two brace
//! groups, only the first two are used.

use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MesError {
    /// A `{` was never closed.
    UnterminatedBrace { line: usize },
    /// The first group of an entry was not an unsigned integer.
    InvalidId { line: usize, text: String },
    /// An id group was not followed by a text group.
    MissingText { line: usize },
}

impl fmt::Display for MesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnterminatedBrace { line } => write!(f, "line {line}: unterminated '{{'"),
            Self::InvalidId { line, text } => write!(f, "line {line}: invalid message id {text:?}"),
            Self::MissingText { line } => write!(f, "line {line}: message id has no text"),
        }
    }
}

impl std::error::Error for MesError {}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MesFile {
    pub messages: HashMap<u32, String>,
}

impl MesFile {
    /// Parse the raw bytes of a `.mes` file. A repeated id replaces the earlier one.
    pub fn parse(data: &[u8]) -> Result<Self, MesError> {
        let mut messages = HashMap::new();
        let mut pos = 0;
        let mut line = 1;

        while let Some((id_text, id_line)) = next_group(data, &mut pos, &mut line)? {
            let id = id_text
                .trim()
                .parse::<u32>()
                .map_err(|_| MesError::InvalidId { line: id_line, text: id_text.clone() })?;
            let (text, _) = next_group(data, &mut pos, &mut line)?
                .ok_or(MesError::MissingText { line: id_line })?;
            messages.insert(id, text);
        }
        Ok(Self { messages })
    }

    pub fn get(&self, id: u32) -> Option<&str> {
        self.messages.get(&id).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

/// Advance to the next `{...}` group, skipping `//` comments and stray text.
/// Returns the decoded contents and the line the group opened on.
fn next_group(
    data: &[u8],
    pos: &mut usize,
    line: &mut usize,
) -> Result<Option<(String, usize)>, MesError> {
    while *pos < data.len() {
        match data[*pos] {
            b'\n' => {
                *line += 1;
                *pos += 1;
            }
            b'/' if data.get(*pos + 1) == Some(&b'/') => {
                while *pos < data.len() && data[*pos] != b'\n' {
                    *pos += 1;
                }
            }
            b'{' => {
                let start_line = *line;
                let start = *pos + 1;
                let mut end = start;
                while end < data.len() && data[end] != b'}' {
                    if data[end] == b'\n' {
                        *line += 1;
                    }
                    end += 1;
                }
                if end >= data.len() {
                    return Err(MesError::UnterminatedBrace { line: start_line });
                }
                *pos = end + 1;
                return Ok(Some((decode_cp1252(&data[start..end]), start_line)));
            }
            _ => *pos += 1,
        }
    }
    Ok(None)
}

const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{8D}', '\u{017D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{9D}', '\u{017E}', '\u{0178}',
];

fn decode_cp1252(bytes: &[u8]) -> String {
    let s: String = bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => CP1252_HIGH[(b - 0x80) as usize],
            _ => b as char, // ASCII and 0xA0..=0xFF map directly to Latin-1
        })
        .collect();
    if s.contains('\r') { s.replace("\r\n", "\n") } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_pairs_and_comments() {
        let m = MesFile::parse(b"// hi\r\n{100}{New Game}\r\n\r\n{101}{Load Game}\r\n").unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m.get(100), Some("New Game"));
        assert_eq!(m.get(101), Some("Load Game"));
    }

    #[test]
    fn multiline_empty_and_trailing_text() {
        let m = MesFile::parse(b"{1}{a\r\n\r\nb}\r\n{2}{} Kite Shield\r\n{3}{x}}\r\n").unwrap();
        assert_eq!(m.get(1), Some("a\n\nb"));
        assert_eq!(m.get(2), Some(""));
        assert_eq!(m.get(3), Some("x"));
    }

    #[test]
    fn comment_inside_braces_is_text() {
        let m = MesFile::parse(b"{1}{a // b}\n").unwrap();
        assert_eq!(m.get(1), Some("a // b"));
    }

    #[test]
    fn decodes_windows_1252() {
        let m = MesFile::parse(b"{1}{I\x92m D\xe9butante}").unwrap();
        assert_eq!(m.get(1), Some("I\u{2019}m D\u{e9}butante"));
    }

    #[test]
    fn errors() {
        assert_eq!(MesFile::parse(b"{1}{abc"), Err(MesError::UnterminatedBrace { line: 1 }));
        assert!(matches!(MesFile::parse(b"\n{x}{y}"), Err(MesError::InvalidId { line: 2, .. })));
        assert_eq!(MesFile::parse(b"{1}"), Err(MesError::MissingText { line: 1 }));
    }
}
