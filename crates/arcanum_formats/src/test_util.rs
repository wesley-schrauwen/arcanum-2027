//! Builds synthetic `.art`, object and sector files so tests do not depend on the game data.

use crate::object::{self, Array, ObjectId, Storage, Value};

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

/// A synthetic bitmap font: 225 static frames `line_height` tall with a grey palette (entry `i`
/// is grey `i`). Frame `n` is `n % 7 + 1` pixels wide, so a glyph's width identifies its frame,
/// and row `r` of every glyph uses palette index `r * 60` (row 0 is empty).
pub fn font_art(line_height: u32) -> Vec<u8> {
    let frame = |n: usize| {
        let w = (n % 7 + 1) as u32;
        let indices: Vec<u8> = (0..line_height).flat_map(|r| std::iter::repeat_n((r * 60).min(255) as u8, w as usize)).collect();
        TestFrame::raw(w, line_height, &indices)
    };
    let mut b = ArtBuilder::single(frame(0));
    b.rotations[0].extend((1..225).map(frame));
    b.palettes = [Some((0..=255u8).map(|i| [i, i, i, 0]).collect()), None, None, None];
    b.build()
}

/// Writes a 24-byte ObjectID: `i16 kind`, 6 pad bytes, the 16-byte payload.
pub fn object_id_bytes(id: &ObjectId) -> [u8; 24] {
    let mut out = [0u8; 24];
    out[..2].copy_from_slice(&id.kind.to_le_bytes());
    out[8..].copy_from_slice(&id.payload);
    out
}

/// A prototype ObjectID for prototype `number`.
pub fn prototype_id(number: u32) -> ObjectId {
    let mut payload = [0u8; 16];
    payload[..4].copy_from_slice(&number.to_le_bytes());
    ObjectId { kind: ObjectId::KIND_PROTOTYPE, payload }
}

/// Writes one field value in its storage encoding. Panics if the value does not fit `storage`.
pub fn value_bytes(storage: Storage, value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    match (storage, value) {
        (Storage::Int32, Value::Int32(v)) => out.extend(v.to_le_bytes()),
        (Storage::Int64, Value::Int64(v)) => match v {
            Some(v) => {
                out.push(1);
                out.extend(v.to_le_bytes());
            }
            None => out.push(0),
        },
        (Storage::ObjectRef, Value::ObjectRef(v)) => match v {
            Some(id) => {
                out.push(1);
                out.extend(object_id_bytes(id));
            }
            None => out.push(0),
        },
        (Storage::String, Value::String(v)) => match v {
            Some(s) => {
                out.push(1);
                out.extend((s.len() as u32).to_le_bytes());
                out.extend(s.as_bytes());
                out.push(0);
            }
            None => out.push(0),
        },
        (Storage::Array, Value::Array(v)) => match v {
            Some(Array { elem_size, id, data, bitset }) => {
                out.push(1);
                out.extend(elem_size.to_le_bytes());
                out.extend((data.len() as u32 / elem_size).to_le_bytes());
                out.extend(id.to_le_bytes());
                out.extend(data);
                out.extend((bitset.len() as u32).to_le_bytes());
                out.extend(bitset.iter().flat_map(|w| w.to_le_bytes()));
            }
            None => out.push(0),
        },
        (s, v) => panic!("{v:?} is not stored as {s:?}"),
    }
    out
}

/// Writes instance objects (the sector / `.mob` / `.mpc` encoding): header, field count, bitmap,
/// then the given fields in bitmap order.
#[derive(Clone, Debug)]
pub struct ObjectBuilder {
    pub object_type: u32,
    pub prototype: ObjectId,
    pub id: ObjectId,
    /// Fields by name; their order here does not matter.
    pub fields: Vec<(&'static str, Value)>,
    /// Bits to set in the bitmap without writing a value (for malformed objects).
    pub extra_bits: Vec<u32>,
}

impl ObjectBuilder {
    /// An object of `object_type` with prototype 1 and an all-zero own id (kind 0, as in sectors).
    pub fn new(object_type: u32) -> Self {
        Self { object_type, prototype: prototype_id(1), id: ObjectId { kind: 0, payload: [0; 16] }, fields: Vec::new(), extra_bits: Vec::new() }
    }

    pub fn field(mut self, name: &'static str, value: Value) -> Self {
        self.fields.push((name, value));
        self
    }

    /// The bytes. Panics if a field name is not a field of the type.
    pub fn build(&self) -> Vec<u8> {
        let bits = object::field_bits(self.object_type).expect("known object type");
        let words = object::bitmap_words(self.object_type).unwrap();
        let mut bitmap = vec![0u32; words];
        let mut values: Vec<(u32, Vec<u8>)> = Vec::new();
        for (name, value) in &self.fields {
            let (bit, def) = bits.iter().find(|(_, d)| d.name == *name).unwrap_or_else(|| panic!("{name} is not a field of type {}", self.object_type));
            bitmap[*bit as usize / 32] |= 1 << (bit % 32);
            values.push((*bit, value_bytes(def.storage, value)));
        }
        for bit in &self.extra_bits {
            bitmap[*bit as usize / 32] |= 1 << (bit % 32);
        }
        values.sort_by_key(|(bit, _)| *bit);
        let mut out = object::VERSION.to_le_bytes().to_vec();
        out.extend(object_id_bytes(&self.prototype));
        out.extend(object_id_bytes(&self.id));
        out.extend(self.object_type.to_le_bytes());
        let count: u32 = bitmap.iter().map(|w| w.count_ones()).sum();
        out.extend((count as u16).to_le_bytes());
        out.extend(bitmap.iter().flat_map(|w| w.to_le_bytes()));
        for (_, v) in values {
            out.extend(v);
        }
        out
    }
}
