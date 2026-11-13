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

/// Horizontal alignment of each line in a block of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
}

/// A tab advances to the next multiple of this many space widths.
pub const TAB_SPACES: u32 = 4;

/// One laid-out line: where it starts (pixels from the left, for tab indents) and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub indent: u32,
    pub text: String,
}

/// Turns the game's escaped `\n` and `\t` (two characters each, as some `.mes` texts write them)
/// into real newlines and tabs, and drops carriage returns.
pub fn unescape(text: &str) -> String {
    text.replace("\\n", "\n").replace("\\t", "\t").replace('\r', "")
}

impl BitmapFont {
    fn space_width(&self) -> u32 {
        self.glyph(' ').map_or(4, |g| g.width).max(1)
    }

    /// Splits `text` into lines: at newlines (real or escaped), and, with `max_width`, wherever
    /// the next word would not fit. Leading tabs indent a line to the next tab stop; a word wider
    /// than the whole line is broken between characters.
    pub fn layout(&self, text: &str, max_width: Option<u32>) -> Vec<Line> {
        let text = unescape(text);
        let tab = self.space_width() * TAB_SPACES;
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            // Leading tabs become an indent; tabs inside the line count as spaces to the next stop.
            let tabs = paragraph.chars().take_while(|c| *c == '\t').count() as u32;
            let indent = tabs * tab;
            let body = &paragraph[tabs as usize..];
            let Some(max) = max_width else {
                lines.push(Line { indent, text: body.replace('\t', " ") });
                continue;
            };
            let mut current = String::new();
            let mut line_indent = indent;
            let first = lines.len();
            for word in body.split([' ', '\t']).filter(|w| !w.is_empty()) {
                let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
                if line_indent + self.measure(&candidate) <= max {
                    current = candidate;
                    continue;
                }
                if !current.is_empty() {
                    lines.push(Line { indent: line_indent, text: std::mem::take(&mut current) });
                    line_indent = 0;
                }
                // The word alone may still be too wide: break it between characters.
                let mut rest = word;
                while line_indent + self.measure(rest) > max {
                    let mut cut = 0;
                    for (i, c) in rest.char_indices() {
                        let end = i + c.len_utf8();
                        if line_indent + self.measure(&rest[..end]) > max {
                            break;
                        }
                        cut = end;
                    }
                    if cut == 0 {
                        // Not even one character fits; place one anyway so layout always advances.
                        cut = rest.chars().next().map_or(rest.len(), char::len_utf8);
                    }
                    lines.push(Line { indent: line_indent, text: rest[..cut].to_string() });
                    line_indent = 0;
                    rest = &rest[cut..];
                    if rest.is_empty() {
                        break;
                    }
                }
                current = rest.to_string();
            }
            // A paragraph always gives at least one line (blank lines are kept), but a word that
            // was broken to the end leaves nothing further to add.
            if !current.is_empty() || lines.len() == first {
                lines.push(Line { indent: line_indent, text: current });
            }
        }
        lines
    }

    /// Renders `text` as a block of lines (see [`Self::layout`]), each `line_height` tall. The
    /// block is as wide as `max_width` when given, otherwise as its widest line.
    pub fn render_block(&self, text: &str, max_width: Option<u32>, align: Align) -> RenderedText {
        self.render_block_limited(text, max_width, align, None)
    }

    /// Like [`Self::render_block`], keeping at most `max_lines` lines (whole lines only).
    pub fn render_block_limited(&self, text: &str, max_width: Option<u32>, align: Align, max_lines: Option<usize>) -> RenderedText {
        let mut lines = self.layout(text, max_width);
        if let Some(n) = max_lines {
            lines.truncate(n.max(1));
        }
        let widths: Vec<u32> = lines.iter().map(|l| l.indent + self.measure(&l.text)).collect();
        let width = max_width.unwrap_or_else(|| widths.iter().copied().max().unwrap_or(0));
        let height = self.line_height * lines.len() as u32;
        let mut alpha = vec![0u8; width as usize * height as usize];
        for (row, (line, line_width)) in lines.iter().zip(&widths).enumerate() {
            let rendered = self.render(&line.text);
            let x0 = match align {
                Align::Left => line.indent,
                Align::Center => width.saturating_sub(*line_width) / 2 + line.indent,
            } as usize;
            let y0 = row * self.line_height as usize;
            for y in 0..rendered.height as usize {
                for x in 0..rendered.width as usize {
                    let dx = x0 + x;
                    if dx >= width as usize {
                        break;
                    }
                    alpha[(y0 + y) * width as usize + dx] = rendered.alpha[y * rendered.width as usize + x];
                }
            }
        }
        RenderedText { width, height, alpha }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{ArtBuilder, TestFrame, font_art};

    /// The test font: glyph widths vary from 1 to 7 pixels.
    fn test_font() -> BitmapFont {
        BitmapFont::parse(&font_art(4)).unwrap()
    }

    fn texts(lines: &[Line]) -> Vec<&str> {
        lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn layout_breaks_at_newlines_real_and_escaped() {
        let f = test_font();
        assert_eq!(texts(&f.layout("a\nb\\nc", None)), ["a", "b", "c"]);
        assert_eq!(texts(&f.layout("one\r\ntwo", None)), ["one", "two"]);
        assert_eq!(texts(&f.layout("a\n\nb", None)), ["a", "", "b"], "blank lines are kept");
    }

    #[test]
    fn leading_tabs_indent_the_line() {
        let f = test_font();
        let space = f.glyph(' ').unwrap().width;
        let lines = f.layout("\tIndented\n\\t\\tTwice", None);
        assert_eq!(lines[0], Line { indent: space * TAB_SPACES, text: "Indented".into() });
        assert_eq!(lines[1].indent, 2 * space * TAB_SPACES);
    }

    #[test]
    fn words_wrap_to_the_width_greedily() {
        let f = test_font();
        let text = "aa bb cc dd ee ff gg hh";
        let max = f.measure("aa bb cc");
        let lines = f.layout(text, Some(max));
        assert_eq!(lines[0].text, "aa bb cc", "as many words as fit");
        for (i, line) in lines.iter().enumerate() {
            assert!(f.measure(&line.text) <= max, "{line:?}");
            // Greedy: the next line's first word would not have fitted on this one.
            if let Some(next) = lines.get(i + 1) {
                let first = next.text.split(' ').next().unwrap();
                assert!(f.measure(&format!("{} {first}", line.text)) > max, "{line:?} then {next:?}");
            }
        }
        assert_eq!(texts(&lines).join(" "), text, "no words lost");
    }

    #[test]
    fn a_word_wider_than_the_line_is_broken() {
        let f = test_font();
        let max = f.measure("abc");
        let lines = f.layout("abcdefgh", Some(max));
        assert_eq!(texts(&lines).concat(), "abcdefgh");
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| f.measure(&l.text) <= max));
        // Even a width narrower than one glyph makes progress.
        assert_eq!(f.layout("ab", Some(1)).len(), 2);
    }

    #[test]
    fn the_first_wrapped_line_keeps_the_indent_and_the_rest_do_not() {
        let f = test_font();
        let tab = f.glyph(' ').unwrap().width * TAB_SPACES;
        let lines = f.layout("\taa bb cc", Some(tab + f.measure("aa bb")));
        assert_eq!(lines.iter().map(|l| l.indent).collect::<Vec<_>>(), [tab, 0]);
        assert_eq!(texts(&lines), ["aa bb", "cc"]);
    }

    #[test]
    fn render_block_stacks_lines_and_aligns_them() {
        let f = test_font();
        let h = f.line_height;
        let block = f.render_block("a\nbbb", None, Align::Left);
        assert_eq!((block.width, block.height), (f.measure("bbb"), 2 * h));
        // Left: the short line starts at x = 0.
        let column = |b: &RenderedText, x: u32| (0..h).any(|y| b.alpha[(y * b.width + x) as usize] > 0);
        assert!(column(&block, 0));
        let centred = f.render_block("a\nbbb", None, Align::Center);
        // Centred: the first line's glyph sits in the middle, not at the left edge.
        let first_row_has_ink_at = |b: &RenderedText, x: u32| (0..h).any(|y| b.alpha[(y * b.width + x) as usize] > 0);
        assert!(!first_row_has_ink_at(&centred, 0) || f.measure("a") == f.measure("bbb"));
        // A fixed width is honoured.
        assert_eq!(f.render_block("a", Some(50), Align::Left).width, 50);
        assert_eq!(f.render_block("", None, Align::Left).height, h, "empty text is one blank line");
        // A line limit keeps whole lines only.
        assert_eq!(f.render_block_limited("a\nb\nc", None, Align::Left, Some(2)).height, 2 * h);
        assert_eq!(f.render_block_limited("a\nb", None, Align::Left, Some(5)).height, 2 * h);
    }

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
