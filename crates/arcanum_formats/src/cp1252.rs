//! Windows-1252, the text encoding of Arcanum's `.mes` files and bitmap fonts.
//!
//! Bytes 0x00-0x7F and 0xA0-0xFF match Unicode (Latin-1). 0x80-0x9F hold typographic
//! characters; the five bytes Windows-1252 leaves undefined map to the C1 control with the
//! same value, so decoding never fails and encoding round-trips.

const HIGH: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{8D}', '\u{017D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{9D}', '\u{017E}', '\u{0178}',
];

/// The character for one Windows-1252 byte.
pub fn decode_byte(b: u8) -> char {
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// The Windows-1252 byte for `c`, or `None` if the encoding cannot represent it.
pub fn encode_char(c: char) -> Option<u8> {
    match c as u32 {
        0x00..=0x7F | 0xA0..=0xFF => Some(c as u8),
        _ => HIGH.iter().position(|&h| h == c).map(|i| 0x80 + i as u8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_byte_round_trips() {
        for b in 0..=255u8 {
            assert_eq!(encode_char(decode_byte(b)), Some(b), "byte {b:#04x}");
        }
    }

    #[test]
    fn typographic_characters() {
        assert_eq!(decode_byte(0x80), '€');
        assert_eq!(encode_char('€'), Some(0x80));
        assert_eq!(encode_char('\u{2019}'), Some(0x92)); // right single quote
        assert_eq!(encode_char('é'), Some(0xE9));
        assert_eq!(encode_char('A'), Some(b'A'));
    }

    #[test]
    fn unrepresentable_characters() {
        for c in ['中', 'ł', '😀', '\u{100}'] {
            assert_eq!(encode_char(c), None, "{c}");
        }
    }
}
