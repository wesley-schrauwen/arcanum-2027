//! Decoder for Troika `.art` sprite files.
//!
//! Layout recovered from the loader in `Arcanum.exe` (`FUN_0051b710`), all
//! little endian:
//!
//! ```text
//! 0x00 u32 flags            bit 0 = static: only rotation 0 is stored
//! 0x04 u32 frame_rate
//! 0x08 u32 rotations        always 8; the loader rejects anything else
//! 0x0c u32 palette_id[4]    non-zero = a 256-entry palette follows
//! 0x1c u32 action_frame
//! 0x20 u32 frame_count      frames per rotation
//! 0x24 u32 info_ptr[8]      runtime pointers in the engine, unused here
//! 0x44 u32 data_size[8]     "
//! 0x64 u32 data_ptr[8]      "
//! 0x84 palettes:            for each present id, 256 x [b, g, r, 0]
//!      frame tables:        for each stored rotation, frame_count x 28 bytes
//!                           { width, height, data_size, center_x, center_y,
//!                             delta_x, delta_y } as u32
//!      pixel data:          for each stored rotation, each frame in order
//! ```
//!
//! A frame whose `data_size == width * height` is raw 8-bit palette indices.
//! Otherwise it is RLE: a control byte with bit 7 set is followed by
//! `(byte & 0x7f)` literal indices; with bit 7 clear it is followed by one
//! index that is repeated `byte` times.
//!
//! Palette index 0 is treated as transparent. The palette alpha byte is
//! always 0 in the data, so this is an assumption, not read from the file.

use std::fmt;

const HEADER_SIZE: usize = 0x84;
const FRAME_INFO_SIZE: usize = 28;
const PALETTE_BYTES: usize = 256 * 4;
const MAX_DIMENSION: u32 = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtError {
    Truncated,
    BadRotationCount(u32),
    NoPalette,
    NoFrames,
    FrameOutOfRange { rotation: usize, frame: usize },
    FrameTooLarge { width: u32, height: u32 },
    BadPaletteIndex(usize),
    /// RLE data decoded to the wrong number of pixels or ran off the end.
    CorruptPixelData,
}

impl fmt::Display for ArtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "file is truncated"),
            Self::BadRotationCount(n) => write!(f, "unexpected rotation count {n} (expected 8)"),
            Self::NoPalette => write!(f, "file has no palette"),
            Self::NoFrames => write!(f, "file has no frames"),
            Self::FrameOutOfRange { rotation, frame } => {
                write!(f, "no frame {frame} in rotation {rotation}")
            }
            Self::FrameTooLarge { width, height } => write!(f, "frame {width}x{height} is too large"),
            Self::BadPaletteIndex(i) => write!(f, "palette {i} is not present"),
            Self::CorruptPixelData => write!(f, "corrupt pixel data"),
        }
    }
}

impl std::error::Error for ArtError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameInfo {
    pub width: u32,
    pub height: u32,
    /// Stored (possibly compressed) size in bytes.
    pub data_size: u32,
    pub center_x: i32,
    pub center_y: i32,
    pub delta_x: i32,
    pub delta_y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtFrame {
    pub info: FrameInfo,
    /// `width * height * 4` bytes, row-major, top row first.
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ArtFile {
    pub flags: u32,
    pub frame_rate: u32,
    pub action_frame: u32,
    pub frame_count: usize,
    /// Rotations stored in the file: 1 for static art, otherwise 8.
    pub rotation_count: usize,
    /// Up to four palettes of 256 `[b, g, r, 0]` entries.
    pub palettes: [Option<Vec<[u8; 4]>>; 4],
    /// `frames[rotation][frame]`.
    pub frames: Vec<Vec<FrameInfo>>,
    /// Offset into `data` where each frame's pixels start, same indexing.
    offsets: Vec<Vec<usize>>,
    data: Vec<u8>,
}

fn u32_at(d: &[u8], off: usize) -> Result<u32, ArtError> {
    d.get(off..off + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or(ArtError::Truncated)
}

impl ArtFile {
    pub fn parse(data: &[u8]) -> Result<Self, ArtError> {
        if data.len() < HEADER_SIZE {
            return Err(ArtError::Truncated);
        }
        let flags = u32_at(data, 0x00)?;
        let frame_rate = u32_at(data, 0x04)?;
        let rotations = u32_at(data, 0x08)?;
        if rotations != 8 {
            return Err(ArtError::BadRotationCount(rotations));
        }
        let action_frame = u32_at(data, 0x1c)?;
        let frame_count = u32_at(data, 0x20)? as usize;
        let rotation_count = if flags & 1 != 0 { 1 } else { 8 };

        let mut pos = HEADER_SIZE;
        let mut palettes: [Option<Vec<[u8; 4]>>; 4] = Default::default();
        for (i, slot) in palettes.iter_mut().enumerate() {
            if u32_at(data, 0x0c + i * 4)? != 0 {
                let raw = data.get(pos..pos + PALETTE_BYTES).ok_or(ArtError::Truncated)?;
                *slot = Some(raw.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect());
                pos += PALETTE_BYTES;
            }
        }

        let table_bytes = frame_count
            .checked_mul(FRAME_INFO_SIZE)
            .ok_or(ArtError::Truncated)?;
        let mut frames = Vec::with_capacity(rotation_count);
        for _ in 0..rotation_count {
            let table = data.get(pos..pos + table_bytes).ok_or(ArtError::Truncated)?;
            frames.push(
                table
                    .chunks_exact(FRAME_INFO_SIZE)
                    .map(|c| {
                        let w = |i: usize| u32::from_le_bytes(c[i * 4..i * 4 + 4].try_into().unwrap());
                        FrameInfo {
                            width: w(0),
                            height: w(1),
                            data_size: w(2),
                            center_x: w(3) as i32,
                            center_y: w(4) as i32,
                            delta_x: w(5) as i32,
                            delta_y: w(6) as i32,
                        }
                    })
                    .collect::<Vec<_>>(),
            );
            pos += table_bytes;
        }

        let mut offsets = Vec::with_capacity(rotation_count);
        for rot in &frames {
            let mut row = Vec::with_capacity(rot.len());
            for f in rot {
                row.push(pos);
                pos = pos.checked_add(f.data_size as usize).ok_or(ArtError::Truncated)?;
            }
            offsets.push(row);
        }
        if pos > data.len() {
            return Err(ArtError::Truncated);
        }

        Ok(Self {
            flags,
            frame_rate,
            action_frame,
            frame_count,
            rotation_count,
            palettes,
            frames,
            offsets,
            data: data.to_vec(),
        })
    }

    /// Frame 0 of rotation 0 using the first palette, as RGBA8 and `(width, height)`.
    pub fn decode_frame0(&self) -> Result<(Vec<u8>, (u32, u32)), ArtError> {
        if self.frame_count == 0 {
            return Err(ArtError::NoFrames);
        }
        let f = self.decode_frame(0, 0, self.default_palette()?)?;
        Ok((f.rgba, (f.info.width, f.info.height)))
    }

    /// Index of the first palette present in the file.
    pub fn default_palette(&self) -> Result<usize, ArtError> {
        self.palettes.iter().position(Option::is_some).ok_or(ArtError::NoPalette)
    }

    pub fn decode_frame(&self, rotation: usize, frame: usize, palette: usize) -> Result<ArtFrame, ArtError> {
        let info = *self
            .frames
            .get(rotation)
            .and_then(|r| r.get(frame))
            .ok_or(ArtError::FrameOutOfRange { rotation, frame })?;
        let pal = self
            .palettes
            .get(palette)
            .and_then(Option::as_ref)
            .ok_or(ArtError::BadPaletteIndex(palette))?;

        if info.width > MAX_DIMENSION || info.height > MAX_DIMENSION {
            return Err(ArtError::FrameTooLarge { width: info.width, height: info.height });
        }
        let pixels = info.width as usize * info.height as usize;
        let start = self.offsets[rotation][frame];
        let stored = &self.data[start..start + info.data_size as usize];

        let indices = if stored.len() == pixels { stored.to_vec() } else { rle_decode(stored, pixels)? };

        let mut rgba = Vec::with_capacity(pixels * 4);
        for &i in &indices {
            let [b, g, r, _] = pal[i as usize];
            rgba.extend_from_slice(&[r, g, b, if i == 0 { 0 } else { 255 }]);
        }
        Ok(ArtFrame { info, rgba })
    }
}

fn rle_decode(src: &[u8], expected: usize) -> Result<Vec<u8>, ArtError> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0;
    while i < src.len() {
        let ctrl = src[i];
        if ctrl & 0x80 != 0 {
            let n = (ctrl & 0x7f) as usize;
            out.extend_from_slice(src.get(i + 1..i + 1 + n).ok_or(ArtError::CorruptPixelData)?);
            i += 1 + n;
        } else {
            let v = *src.get(i + 1).ok_or(ArtError::CorruptPixelData)?;
            out.resize(out.len() + ctrl as usize, v);
            i += 2;
        }
        if out.len() > expected {
            return Err(ArtError::CorruptPixelData);
        }
    }
    if out.len() != expected {
        return Err(ArtError::CorruptPixelData);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{ArtBuilder, TestFrame, ramp_palette, rle_encode};

    /// RGBA for palette index `i` of `ramp_palette()`.
    fn ramp_rgba(i: u8) -> [u8; 4] {
        let [b, g, r, _] = ramp_palette()[i as usize];
        [r, g, b, if i == 0 { 0 } else { 255 }]
    }

    fn expected_rgba(indices: &[u8]) -> Vec<u8> {
        indices.iter().flat_map(|&i| ramp_rgba(i)).collect()
    }

    /// Deterministic pseudo-random bytes with long runs mixed in.
    fn noisy_indices(len: usize, seed: u32) -> Vec<u8> {
        let mut x = seed.wrapping_mul(2_654_435_761).max(1);
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            let run = if x % 4 == 0 { (x >> 8) as usize % 300 + 1 } else { 1 };
            out.extend(std::iter::repeat_n((x >> 16) as u8, run.min(len - out.len())));
        }
        out
    }

    #[test]
    fn raw_frame_decodes_to_rgba() {
        let indices = [0, 1, 2, 255, 128, 7];
        let art = ArtFile::parse(&ArtBuilder::single(TestFrame::raw(3, 2, &indices)).build()).unwrap();
        let (rgba, dims) = art.decode_frame0().unwrap();
        assert_eq!(dims, (3, 2));
        assert_eq!(rgba, expected_rgba(&indices));
    }

    #[test]
    fn index_zero_is_transparent() {
        let art = ArtFile::parse(&ArtBuilder::single(TestFrame::raw(2, 1, &[0, 1])).build()).unwrap();
        let (rgba, _) = art.decode_frame0().unwrap();
        assert_eq!(rgba[3], 0);
        assert_eq!(rgba[7], 255);
    }

    #[test]
    fn rle_frame_decodes_to_rgba() {
        let indices: Vec<u8> = [vec![5; 40], vec![1, 2, 3], vec![9; 17]].concat();
        let art = ArtFile::parse(&ArtBuilder::single(TestFrame::rle(10, 6, &indices)).build()).unwrap();
        let (rgba, dims) = art.decode_frame0().unwrap();
        assert_eq!(dims, (10, 6));
        assert_eq!(rgba, expected_rgba(&indices));
    }

    #[test]
    fn rle_round_trips() {
        for (len, seed) in [(1, 1), (2, 2), (127, 3), (128, 4), (129, 5), (1000, 6), (20_000, 7)] {
            let indices = noisy_indices(len, seed);
            assert_eq!(rle_decode(&rle_encode(&indices), len).unwrap(), indices, "len {len}");
        }
        assert_eq!(rle_decode(&rle_encode(&[3; 1000]), 1000).unwrap(), vec![3; 1000]);
    }

    #[test]
    fn rle_zero_length_runs_are_accepted() {
        // A zero-count run and a zero-count literal emit nothing.
        assert_eq!(rle_decode(&[0, 9, 0x80, 2, 4], 2).unwrap(), vec![4, 4]);
    }

    #[test]
    fn rle_errors() {
        assert_eq!(rle_decode(&[0x83, 1, 2], 3), Err(ArtError::CorruptPixelData)); // literal overruns
        assert_eq!(rle_decode(&[3], 3), Err(ArtError::CorruptPixelData)); // run missing its value
        assert_eq!(rle_decode(&[3, 1], 4), Err(ArtError::CorruptPixelData)); // too short
        assert_eq!(rle_decode(&[3, 1, 3, 1], 4), Err(ArtError::CorruptPixelData)); // too long
    }

    #[test]
    fn animated_art_indexes_rotations_and_frames() {
        // Each frame is filled with a distinct index and has distinct offsets.
        let builder = ArtBuilder::animated(3, |r, f| {
            let v = (r * 10 + f + 1) as u8;
            let frame = if f % 2 == 0 { TestFrame::raw(4, 2, &[v; 8]) } else { TestFrame::rle(4, 2, &[v; 8]) };
            frame.with_offsets((r as i32, -(f as i32)), (-1, 2))
        });
        let art = ArtFile::parse(&builder.build()).unwrap();
        assert_eq!((art.rotation_count, art.frame_count), (8, 3));
        assert_eq!(art.frame_rate, 10);
        for r in 0..8 {
            for f in 0..3 {
                let frame = art.decode_frame(r, f, 0).unwrap();
                let v = (r * 10 + f + 1) as u8;
                assert_eq!(frame.rgba, expected_rgba(&[v; 8]), "rotation {r} frame {f}");
                assert_eq!((frame.info.center_x, frame.info.center_y), (r as i32, -(f as i32)));
                assert_eq!((frame.info.delta_x, frame.info.delta_y), (-1, 2));
            }
        }
        assert_eq!(art.decode_frame(8, 0, 0).unwrap_err(), ArtError::FrameOutOfRange { rotation: 8, frame: 0 });
        assert_eq!(art.decode_frame(0, 3, 0).unwrap_err(), ArtError::FrameOutOfRange { rotation: 0, frame: 3 });
    }

    #[test]
    fn static_flag_stores_one_rotation() {
        let art = ArtFile::parse(&ArtBuilder::single(TestFrame::raw(1, 1, &[1])).build()).unwrap();
        assert_eq!(art.rotation_count, 1);
        assert_eq!(art.decode_frame(1, 0, 0).unwrap_err(), ArtError::FrameOutOfRange { rotation: 1, frame: 0 });
    }

    #[test]
    fn palette_selection() {
        let mut b = ArtBuilder::single(TestFrame::raw(1, 1, &[1]));
        let red: Vec<[u8; 4]> = vec![[0, 0, 255, 0]; 256];
        b.palettes = [None, None, Some(ramp_palette()), Some(red)];
        let art = ArtFile::parse(&b.build()).unwrap();
        assert_eq!(art.default_palette(), Ok(2));
        assert_eq!(art.decode_frame0().unwrap().0, ramp_rgba(1));
        assert_eq!(art.decode_frame(0, 0, 3).unwrap().rgba, vec![255, 0, 0, 255]);
        assert_eq!(art.decode_frame(0, 0, 0).unwrap_err(), ArtError::BadPaletteIndex(0));
        assert_eq!(art.decode_frame(0, 0, 9).unwrap_err(), ArtError::BadPaletteIndex(9));

        b.palettes = [None, None, None, None];
        let art = ArtFile::parse(&b.build()).unwrap();
        assert_eq!(art.decode_frame0().unwrap_err(), ArtError::NoPalette);
    }

    #[test]
    fn empty_and_zero_sized_frames() {
        let mut b = ArtBuilder::single(TestFrame::raw(1, 1, &[1]));
        b.rotations = vec![vec![]];
        let art = ArtFile::parse(&b.build()).unwrap();
        assert_eq!(art.decode_frame0().unwrap_err(), ArtError::NoFrames);

        let art = ArtFile::parse(&ArtBuilder::single(TestFrame::raw(0, 0, &[])).build()).unwrap();
        assert_eq!(art.decode_frame0().unwrap(), (vec![], (0, 0)));
    }

    #[test]
    fn rejects_malformed_headers() {
        let good = ArtBuilder::single(TestFrame::raw(2, 2, &[1, 2, 3, 4])).build();
        assert_eq!(ArtFile::parse(&good[..10]).unwrap_err(), ArtError::Truncated);
        assert_eq!(ArtFile::parse(&good[..HEADER_SIZE + 100]).unwrap_err(), ArtError::Truncated); // in palette
        assert_eq!(ArtFile::parse(&good[..HEADER_SIZE + PALETTE_BYTES + 10]).unwrap_err(), ArtError::Truncated); // in table
        assert_eq!(ArtFile::parse(&good[..good.len() - 1]).unwrap_err(), ArtError::Truncated); // in pixels

        let mut bad = good.clone();
        bad[8] = 4;
        assert_eq!(ArtFile::parse(&bad).unwrap_err(), ArtError::BadRotationCount(4));

        let mut huge = good.clone();
        huge[0x20..0x24].copy_from_slice(&u32::MAX.to_le_bytes()); // frame count
        assert_eq!(ArtFile::parse(&huge).unwrap_err(), ArtError::Truncated);
    }

    #[test]
    fn rejects_oversized_frames() {
        let art = ArtFile::parse(&ArtBuilder::single(TestFrame::stored(9000, 1, vec![2, 1])).build()).unwrap();
        assert_eq!(art.decode_frame0().unwrap_err(), ArtError::FrameTooLarge { width: 9000, height: 1 });
    }

    #[test]
    fn corrupt_pixels_are_errors_not_panics() {
        let art = ArtFile::parse(&ArtBuilder::single(TestFrame::stored(3, 3, vec![0x85, 1, 2])).build()).unwrap();
        assert_eq!(art.decode_frame0().unwrap_err(), ArtError::CorruptPixelData);
    }
}
