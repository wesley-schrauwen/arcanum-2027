//! Builds synthetic `.art` files so tests do not depend on the game data.

/// One stored frame: its header fields and the bytes exactly as they appear in the file.
#[derive(Clone, Debug)]
pub struct TestFrame {
    pub width: u32,
    pub height: u32,
    pub center: (i32, i32),
    pub delta: (i32, i32),
    pub stored: Vec<u8>,
}

impl TestFrame {
    /// Uncompressed palette indices.
    pub fn raw(width: u32, height: u32, indices: &[u8]) -> Self {
        assert_eq!(indices.len(), (width * height) as usize);
        Self { width, height, center: (0, 0), delta: (0, 0), stored: indices.to_vec() }
    }

    /// RLE-encoded palette indices. Panics if the encoding happens to be `width * height`
    /// bytes long, since the format would then read it as raw.
    pub fn rle(width: u32, height: u32, indices: &[u8]) -> Self {
        assert_eq!(indices.len(), (width * height) as usize);
        let stored = rle_encode(indices);
        assert_ne!(stored.len(), indices.len(), "RLE output would be read as raw");
        Self { width, height, center: (0, 0), delta: (0, 0), stored }
    }

    /// Arbitrary stored bytes, for malformed frames.
    pub fn stored(width: u32, height: u32, stored: Vec<u8>) -> Self {
        Self { width, height, center: (0, 0), delta: (0, 0), stored }
    }

    pub fn with_offsets(mut self, center: (i32, i32), delta: (i32, i32)) -> Self {
        self.center = center;
        self.delta = delta;
        self
    }
}

/// Encodes palette indices with the `.art` RLE scheme: `0x80 | n` + n literals, or
/// `n` + value for a run of n (n < 128).
pub fn rle_encode(indices: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < indices.len() {
        let run = indices[i..].iter().take(127).take_while(|&&v| v == indices[i]).count();
        if run >= 3 {
            out.extend_from_slice(&[run as u8, indices[i]]);
            i += run;
            continue;
        }
        // Literals until the next run of 3 or the 127-byte limit.
        let start = i;
        while i < indices.len() && i - start < 127 {
            let ahead = &indices[i..];
            if ahead.len() >= 3 && ahead[0] == ahead[1] && ahead[1] == ahead[2] {
                break;
            }
            i += 1;
        }
        out.push(0x80 | (i - start) as u8);
        out.extend_from_slice(&indices[start..i]);
    }
    out
}

/// A palette where entry `i` is stored as `[b, g, r, 0] = [i, 255 - i, i / 2, 0]`.
pub fn ramp_palette() -> Vec<[u8; 4]> {
    (0..=255u8).map(|i| [i, 255 - i, i / 2, 0]).collect()
}

/// Writes `.art` files in the layout `ArtFile` reads.
#[derive(Clone, Debug)]
pub struct ArtBuilder {
    pub flags: u32,
    pub frame_rate: u32,
    pub action_frame: u32,
    pub palettes: [Option<Vec<[u8; 4]>>; 4],
    /// One entry per stored rotation: 1 for static art, 8 otherwise.
    pub rotations: Vec<Vec<TestFrame>>,
}

impl ArtBuilder {
    /// Static art (flag bit 0): one rotation, one palette in slot 0.
    pub fn single(frame: TestFrame) -> Self {
        Self {
            flags: 1,
            frame_rate: 0,
            action_frame: 0,
            palettes: [Some(ramp_palette()), None, None, None],
            rotations: vec![vec![frame]],
        }
    }

    /// Animated art: eight rotations, each with the frames `frame(rotation, index)` returns.
    pub fn animated(frames: usize, frame: impl Fn(usize, usize) -> TestFrame) -> Self {
        Self {
            flags: 0,
            frame_rate: 10,
            action_frame: 0,
            palettes: [Some(ramp_palette()), None, None, None],
            rotations: (0..8).map(|r| (0..frames).map(|f| frame(r, f)).collect()).collect(),
        }
    }

    pub fn build(&self) -> Vec<u8> {
        let frame_count = self.rotations.first().map_or(0, Vec::len);
        assert!(self.rotations.iter().all(|r| r.len() == frame_count));

        let mut header = [0u32; 33];
        header[0] = self.flags;
        header[1] = self.frame_rate;
        header[2] = 8;
        for (i, p) in self.palettes.iter().enumerate() {
            header[3 + i] = if p.is_some() { i as u32 + 1 } else { 0 };
        }
        header[7] = self.action_frame;
        header[8] = frame_count as u32;

        let mut out: Vec<u8> = header.iter().flat_map(|v| v.to_le_bytes()).collect();
        for p in self.palettes.iter().flatten() {
            assert_eq!(p.len(), 256);
            out.extend(p.iter().flatten());
        }
        for rotation in &self.rotations {
            for f in rotation {
                let fields = [
                    f.width,
                    f.height,
                    f.stored.len() as u32,
                    f.center.0 as u32,
                    f.center.1 as u32,
                    f.delta.0 as u32,
                    f.delta.1 as u32,
                ];
                out.extend(fields.iter().flat_map(|v| v.to_le_bytes()));
            }
        }
        for rotation in &self.rotations {
            for f in rotation {
                out.extend_from_slice(&f.stored);
            }
        }
        out
    }
}
