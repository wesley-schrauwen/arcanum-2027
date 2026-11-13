//! Arcanum bitmap fonts: `.art` files such as `art/interface/Morph30Font.ART`.
//!
//! A font is static art with 225 frames and one palette. Frame 0 is the text cursor; frame
//! `n` (1..=224) is the glyph for Windows-1252 byte `n + 31`, so frame 1 is the space and the
//! last frame is `ÿ` (0xFF). Every glyph has the font's line height. Pixels are palette indices
//! into a greyscale ramp: index 0 is empty and the grey level of the others is anti-aliasing
//! coverage, so a glyph is an alpha mask that can be tinted any colour.
//!
//! Glyphs are drawn edge to edge: a glyph's width is its advance.

use std::fmt;

use crate::art::{ArtError, ArtFile};
use crate::cp1252;

/// Frames in a font file: the cursor plus one glyph per byte from 0x20 to 0xFF.
pub const FONT_FRAMES: usize = 225;
/// The byte drawn by frame 1.
pub const FIRST_CHAR: u8 = 0x20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontError {
    Art(ArtError),
    /// Not shaped like a font: wrong frame count, animated, or uneven glyph heights.
    NotAFont(String),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Art(e) => write!(f, "{e}"),
            Self::NotAFont(why) => write!(f, "not a bitmap font: {why}"),
        }
    }
}

impl std::error::Error for FontError {}

impl From<ArtError> for FontError {
    fn from(e: ArtError) -> Self {
        Self::Art(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    pub width: u32,
    /// Coverage, `width * line_height` bytes, row-major, 0 = empty, 255 = solid.
    pub alpha: Vec<u8>,
}

/// One line of rendered text: a coverage mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedText {
    pub width: u32,
    pub height: u32,
    /// `width * height` coverage bytes, row-major.
    pub alpha: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitmapFont {
    pub line_height: u32,
    pub cursor: Glyph,
    /// Glyphs for bytes `FIRST_CHAR..=0xFF`.
    glyphs: Vec<Glyph>,
}

impl BitmapFont {
    pub fn parse(data: &[u8]) -> Result<Self, FontError> {
        Self::from_art(&ArtFile::parse(data)?)
    }

    pub fn from_art(art: &ArtFile) -> Result<Self, FontError> {
        if art.rotation_count != 1 {
            return Err(FontError::NotAFont("animated art".into()));
        }
        if art.frame_count != FONT_FRAMES {
            return Err(FontError::NotAFont(format!("{} frames, expected {FONT_FRAMES}", art.frame_count)));
        }
        let palette = art.default_palette()?;
        let line_height = art.frames[0][0].height;
        let mut frames = Vec::with_capacity(FONT_FRAMES);
        for index in 0..FONT_FRAMES {
            let frame = art.decode_frame(0, index, palette)?;
            if frame.info.height != line_height {
                return Err(FontError::NotAFont(format!(
                    "glyph {index} is {} pixels tall, line height is {line_height}",
                    frame.info.height
                )));
            }
            // The palette is a grey ramp: any channel gives the coverage. Index 0 decodes with
            // alpha 0, which marks empty pixels.
            let alpha = frame.rgba.chunks_exact(4).map(|p| if p[3] == 0 { 0 } else { p[0].max(p[1]).max(p[2]) }).collect();
            frames.push(Glyph { width: frame.info.width, alpha });
        }
        let cursor = frames.remove(0);
        Ok(Self { line_height, cursor, glyphs: frames })
    }

    /// The glyph for a Windows-1252 byte; `None` for control bytes below 0x20.
    pub fn glyph_for_byte(&self, b: u8) -> Option<&Glyph> {
        b.checked_sub(FIRST_CHAR).map(|i| &self.glyphs[i as usize])
    }

    /// The glyph for `c`; `None` if Windows-1252 cannot represent it or it is a control character.
    pub fn glyph(&self, c: char) -> Option<&Glyph> {
        cp1252::encode_char(c).and_then(|b| self.glyph_for_byte(b))
    }

    /// Glyphs to draw for `text`. Characters with no glyph are drawn as `?`.
    fn glyphs_for<'a>(&'a self, text: &'a str) -> impl Iterator<Item = &'a Glyph> + 'a {
        let fallback = self.glyph('?').expect("fonts always contain '?'");
        text.chars().map(move |c| self.glyph(c).unwrap_or(fallback))
    }

    /// Width in pixels of `text` on one line.
    pub fn measure(&self, text: &str) -> u32 {
        self.glyphs_for(text).map(|g| g.width).sum()
    }

    /// Renders `text` on one line as a coverage mask `line_height` pixels tall.
    pub fn render(&self, text: &str) -> RenderedText {
        let width = self.measure(text);
        let height = self.line_height;
        let mut alpha = vec![0u8; width as usize * height as usize];
        let mut x = 0usize;
        for glyph in self.glyphs_for(text) {
            let w = glyph.width as usize;
            for row in 0..height as usize {
                let dst = row * width as usize + x;
                alpha[dst..dst + w].copy_from_slice(&glyph.alpha[row * w..row * w + w]);
            }
            x += w;
        }
        RenderedText { width, height, alpha }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{ArtBuilder, TestFrame, font_art};

    #[test]
    fn maps_bytes_to_frames() {
        let font = BitmapFont::parse(&font_art(4)).unwrap();
        assert_eq!(font.line_height, 4);
        // font_art makes glyph n (frame n) n % 7 + 1 pixels wide, so the width identifies the frame.
        let frame_of = |c: char| font.glyph(c).map(|g| g.width);
        assert_eq!(frame_of(' '), Some(1 % 7 + 1));
        assert_eq!(frame_of('!'), Some(2 % 7 + 1));
        assert_eq!(frame_of('A'), Some((b'A' - 31) as u32 % 7 + 1));
        assert_eq!(frame_of('€'), Some((0x80 - 31) % 7 + 1));
        assert_eq!(frame_of('ÿ'), Some((0xFF - 31) % 7 + 1));
        assert_eq!(font.cursor.width, 1);
    }

    #[test]
    fn unsupported_characters() {
        let font = BitmapFont::parse(&font_art(4)).unwrap();
        assert!(font.glyph('\n').is_none());
        assert!(font.glyph('中').is_none());
        // Rendering substitutes '?'.
        assert_eq!(font.measure("中"), font.measure("?"));
    }

    #[test]
    fn measure_sums_advances() {
        let font = BitmapFont::parse(&font_art(4)).unwrap();
        let w = |c| font.glyph(c).unwrap().width;
        assert_eq!(font.measure("Ab c"), w('A') + w('b') + w(' ') + w('c'));
        assert_eq!(font.measure(""), 0);
    }

    #[test]
    fn coverage_comes_from_the_grey_ramp() {
        let font = BitmapFont::parse(&font_art(4)).unwrap();
        // font_art fills glyph rows with palette indices 0, 1, 2, ... and the palette is grey i.
        let g = font.glyph('A').unwrap();
        for row in 0..4usize {
            let expected = if row == 0 { 0 } else { row as u8 * 60 };
            assert!(g.alpha[row * g.width as usize..(row + 1) * g.width as usize].iter().all(|&a| a == expected), "row {row}");
        }
    }

    #[test]
    fn render_places_glyphs_side_by_side() {
        let font = BitmapFont::parse(&font_art(4)).unwrap();
        let text = font.render("AB");
        let (a, b) = (font.glyph('A').unwrap(), font.glyph('B').unwrap());
        assert_eq!((text.width, text.height), (a.width + b.width, 4));
        for row in 0..4usize {
            let line = &text.alpha[row * text.width as usize..(row + 1) * text.width as usize];
            assert_eq!(&line[..a.width as usize], &a.alpha[row * a.width as usize..(row + 1) * a.width as usize]);
            assert_eq!(&line[a.width as usize..], &b.alpha[row * b.width as usize..(row + 1) * b.width as usize]);
        }
        assert_eq!(font.render(""), RenderedText { width: 0, height: 4, alpha: vec![] });
    }

    #[test]
    fn rejects_art_that_is_not_a_font() {
        let sprite = ArtBuilder::single(TestFrame::raw(2, 2, &[1, 2, 3, 4])).build();
        assert!(matches!(BitmapFont::parse(&sprite), Err(FontError::NotAFont(_))));

        let animated = ArtBuilder::animated(FONT_FRAMES, |_, _| TestFrame::raw(1, 1, &[1])).build();
        assert!(matches!(BitmapFont::parse(&animated), Err(FontError::NotAFont(_))));

        let mut uneven = ArtBuilder::single(TestFrame::raw(1, 3, &[1, 1, 1]));
        uneven.rotations[0].extend((1..FONT_FRAMES).map(|i| TestFrame::raw(1, if i == 50 { 2 } else { 3 }, &vec![1; if i == 50 { 2 } else { 3 }])));
        assert!(matches!(BitmapFont::parse(&uneven.build()), Err(FontError::NotAFont(_))));

        assert!(matches!(BitmapFont::parse(b"junk"), Err(FontError::Art(_))));
    }
}
