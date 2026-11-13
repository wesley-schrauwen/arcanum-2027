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

fn decode_cp1252(bytes: &[u8]) -> String {
    let s: String = bytes.iter().map(|&b| crate::cp1252::decode_byte(b)).collect();
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
    fn empty_and_comment_only_files() {
        assert!(MesFile::parse(b"").unwrap().is_empty());
        assert!(MesFile::parse(b"// nothing\r\n//{1}{commented out}\r\n\r\n").unwrap().is_empty());
    }

    #[test]
    fn whitespace_and_layout_between_groups() {
        let m = MesFile::parse(b"{ 7 }\t{seven}   {8}\n\n{eight}").unwrap();
        assert_eq!(m.get(7), Some("seven"));
        assert_eq!(m.get(8), Some("eight"));
    }

    #[test]
    fn text_is_kept_verbatim() {
        let m = MesFile::parse(b"{1}{  padded  }{2}{50% off, %d coins}{3}{a{b}").unwrap();
        assert_eq!(m.get(1), Some("  padded  "));
        assert_eq!(m.get(2), Some("50% off, %d coins"));
        assert_eq!(m.get(3), Some("a{b")); // braces do not nest
    }

    #[test]
    fn single_slash_outside_braces_is_ignored() {
        let m = MesFile::parse(b"/ {1}{x}").unwrap();
        assert_eq!(m.get(1), Some("x"));
    }

    #[test]
    fn duplicate_ids_keep_the_last() {
        let m = MesFile::parse(b"{1}{first}\n{1}{second}").unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m.get(1), Some("second"));
    }

    #[test]
    fn extra_groups_on_an_entry_are_paired_in_order() {
        // Groups are read strictly in pairs, as the format has no entry terminator.
        let m = MesFile::parse(b"{1}{a}{2}{b}").unwrap();
        assert_eq!((m.get(1), m.get(2)), (Some("a"), Some("b")));
    }

    #[test]
    fn large_ids() {
        let m = MesFile::parse(b"{4294967295}{max}").unwrap();
        assert_eq!(m.get(u32::MAX), Some("max"));
        assert!(matches!(MesFile::parse(b"{4294967296}{too big}"), Err(MesError::InvalidId { .. })));
        assert!(matches!(MesFile::parse(b"{-1}{neg}"), Err(MesError::InvalidId { .. })));
    }

    #[test]
    fn windows_1252_table() {
        let m = MesFile::parse(b"{1}{\x80\x85\x93\x94\x96\x97\x99}{2}{\x81\x8d\x8f\x90\x9d}{3}{\xa0\xe9\xff}").unwrap();
        assert_eq!(m.get(1), Some("\u{20AC}\u{2026}\u{201C}\u{201D}\u{2013}\u{2014}\u{2122}"));
        // Bytes undefined in Windows-1252 pass through as the matching C1 control.
        assert_eq!(m.get(2), Some("\u{81}\u{8D}\u{8F}\u{90}\u{9D}"));
        assert_eq!(m.get(3), Some("\u{A0}\u{E9}\u{FF}"));
    }

    #[test]
    fn lone_cr_is_kept() {
        let m = MesFile::parse(b"{1}{a\rb}").unwrap();
        assert_eq!(m.get(1), Some("a\rb"));
    }

    #[test]
    fn error_lines_count_newlines_inside_text() {
        assert!(matches!(
            MesFile::parse(b"{1}{line one\nline two\nline three}\n{oops}{x}"),
            Err(MesError::InvalidId { line: 4, .. })
        ));
        assert_eq!(MesFile::parse(b"\n\n{1}{a}\n{2}"), Err(MesError::MissingText { line: 4 }));
        assert_eq!(MesFile::parse(b"{1}{a}\n\n{2}{unterminated\n"), Err(MesError::UnterminatedBrace { line: 3 }));
    }

    #[test]
    fn errors_display_line_numbers() {
        let e = MesFile::parse(b"\n{x}{y}").unwrap_err();
        assert_eq!(e.to_string(), "line 2: invalid message id \"x\"");
    }

    #[test]
    fn errors() {
        assert_eq!(MesFile::parse(b"{1}{abc"), Err(MesError::UnterminatedBrace { line: 1 }));
        assert!(matches!(MesFile::parse(b"\n{x}{y}"), Err(MesError::InvalidId { line: 2, .. })));
        assert_eq!(MesFile::parse(b"{1}"), Err(MesError::MissingText { line: 1 }));
    }
}
