//! Arcanum's binary object format: prototypes (`proto/*.pro`), pre-made characters
//! (`Players/*.mpc`), sector objects (`maps/*/*.sec`) and mobile objects (`maps/*/*.mob`).
//!
//! Every object is a header plus a list of fields. The field list per object type and its order
//! come from the engine's own `obj_f_*` enum (the names are in Arcanum.exe); the tables are in
//! `object_fields.rs`, generated with the engine's storage kind per field. Each field has one of
//! five storage kinds; see [`Storage`]. Two layouts exist:
//! - **Prototype**: header, a 20-byte "fields set" mask, then *every* field of the type in order.
//!   Only player characters (type 15) are read this way so far.
//! - **Instance** (`FUN_00409aa0` at 0x409aa0, via the reader `FUN_00406600` at 0x406600), every
//!   type:
//!
//! | Offset | Size | Meaning |
//! |---|---|---|
//! | 0 | 4 | version = 0x77 |
//! | 4 | 24 | prototype ObjectID (kind 1 in every shipped instance) |
//! | 28 | 24 | own ObjectID (kind 0 in sector objects, 2 = GUID in `.mob` and `.mpc`) |
//! | 52 | 4 | type, 0..=17 (`TYPE_*`) |
//! | 56 | 2 | stored field count (== popcount of the bitmap) |
//! | 58 | 4 * W | bitmap, W = [`bitmap_words`] (3, 4 for items, 5 for critters) |
//! | 58+4W | ... | values of the set bits, in increasing bit order |
//!
//! A type is a list of field blocks ([`field_blocks`]): common, then wall / portal / container /
//! scenery / projectile / trap; or item then the item subtype; or critter then pc / npc. Bit of a
//! field = the block's first bit + its position in the block. The common block starts at bit 0
//! and each later block at the next 32-bit boundary after the previous block's last field: common
//! (36 fields) then 64; item (23) then 96; critter (33) then 128. W = word of the last field + 1
//! (`FUN_0040c030` at 0x40c030; field ranges per type in `FUN_0040cba0` at 0x40cba0). Storage
//! kinds come from the engine's static field table (`FUN_0040a8a0`..0x40b8d5, read by
//! `FUN_004e44f0`; sparse arrays by `FUN_004e78f0`). See NOTES.md ("Object format").

use std::fmt;

use crate::cp1252;
pub use crate::object_fields::*;

/// Format version written by the shipped game.
pub const VERSION: u32 = 0x77;

/// Object types, numbered as the engine numbers them.
pub const TYPE_WALL: u32 = 0;
pub const TYPE_PORTAL: u32 = 1;
pub const TYPE_CONTAINER: u32 = 2;
pub const TYPE_SCENERY: u32 = 3;
pub const TYPE_PROJECTILE: u32 = 4;
pub const TYPE_WEAPON: u32 = 5;
pub const TYPE_AMMO: u32 = 6;
pub const TYPE_ARMOR: u32 = 7;
pub const TYPE_GOLD: u32 = 8;
pub const TYPE_FOOD: u32 = 9;
pub const TYPE_SCROLL: u32 = 10;
pub const TYPE_KEY: u32 = 11;
pub const TYPE_KEY_RING: u32 = 12;
pub const TYPE_WRITTEN: u32 = 13;
pub const TYPE_GENERIC: u32 = 14;
/// Object type of a player character.
pub const TYPE_PC: u32 = 15;
pub const TYPE_NPC: u32 = 16;
pub const TYPE_TRAP: u32 = 17;

type Blocks = &'static [&'static [FieldDef]];
const WALL_BLOCKS: Blocks = &[&COMMON_FIELDS, &WALL_FIELDS];
const PORTAL_BLOCKS: Blocks = &[&COMMON_FIELDS, &PORTAL_FIELDS];
const CONTAINER_BLOCKS: Blocks = &[&COMMON_FIELDS, &CONTAINER_FIELDS];
const SCENERY_BLOCKS: Blocks = &[&COMMON_FIELDS, &SCENERY_FIELDS];
const PROJECTILE_BLOCKS: Blocks = &[&COMMON_FIELDS, &PROJECTILE_FIELDS];
const WEAPON_BLOCKS: Blocks = &[&COMMON_FIELDS, &ITEM_FIELDS, &WEAPON_FIELDS];
const AMMO_BLOCKS: Blocks = &[&COMMON_FIELDS, &ITEM_FIELDS, &AMMO_FIELDS];
const ARMOR_BLOCKS: Blocks = &[&COMMON_FIELDS, &ITEM_FIELDS, &ARMOR_FIELDS];
const GOLD_BLOCKS: Blocks = &[&COMMON_FIELDS, &ITEM_FIELDS, &GOLD_FIELDS];
const FOOD_BLOCKS: Blocks = &[&COMMON_FIELDS, &ITEM_FIELDS, &FOOD_FIELDS];
const SCROLL_BLOCKS: Blocks = &[&COMMON_FIELDS, &ITEM_FIELDS, &SCROLL_FIELDS];
const KEY_BLOCKS: Blocks = &[&COMMON_FIELDS, &ITEM_FIELDS, &KEY_FIELDS];
const KEY_RING_BLOCKS: Blocks = &[&COMMON_FIELDS, &ITEM_FIELDS, &KEY_RING_FIELDS];
const WRITTEN_BLOCKS: Blocks = &[&COMMON_FIELDS, &ITEM_FIELDS, &WRITTEN_FIELDS];
const GENERIC_BLOCKS: Blocks = &[&COMMON_FIELDS, &ITEM_FIELDS, &GENERIC_FIELDS];
const PC_BLOCKS: Blocks = &[&COMMON_FIELDS, &CRITTER_FIELDS, &PC_FIELDS];
const NPC_BLOCKS: Blocks = &[&COMMON_FIELDS, &CRITTER_FIELDS, &NPC_FIELDS];
const TRAP_BLOCKS: Blocks = &[&COMMON_FIELDS, &TRAP_FIELDS];

/// The field blocks of `object_type` in bitmap order, or `None` for an unknown type.
pub fn field_blocks(object_type: u32) -> Option<&'static [&'static [FieldDef]]> {
    Some(match object_type {
        TYPE_WALL => WALL_BLOCKS,
        TYPE_PORTAL => PORTAL_BLOCKS,
        TYPE_CONTAINER => CONTAINER_BLOCKS,
        TYPE_SCENERY => SCENERY_BLOCKS,
        TYPE_PROJECTILE => PROJECTILE_BLOCKS,
        TYPE_WEAPON => WEAPON_BLOCKS,
        TYPE_AMMO => AMMO_BLOCKS,
        TYPE_ARMOR => ARMOR_BLOCKS,
        TYPE_GOLD => GOLD_BLOCKS,
        TYPE_FOOD => FOOD_BLOCKS,
        TYPE_SCROLL => SCROLL_BLOCKS,
        TYPE_KEY => KEY_BLOCKS,
        TYPE_KEY_RING => KEY_RING_BLOCKS,
        TYPE_WRITTEN => WRITTEN_BLOCKS,
        TYPE_GENERIC => GENERIC_BLOCKS,
        TYPE_PC => PC_BLOCKS,
        TYPE_NPC => NPC_BLOCKS,
        TYPE_TRAP => TRAP_BLOCKS,
        _ => return None,
    })
}

/// Each field of `blocks` with its bitmap bit: a block starts at the next 32-bit boundary after
/// the previous block's last field.
fn with_bits(blocks: Blocks) -> impl Iterator<Item = (u32, &'static FieldDef)> {
    blocks.iter().scan(0u32, |next, block| {
        let first = *next;
        *next = (first + block.len() as u32).div_ceil(32) * 32;
        Some(block.iter().enumerate().map(move |(i, d)| (first + i as u32, d)))
    })
    .flatten()
}

/// Every field of `object_type` with its bitmap bit, in bit order.
pub fn field_bits(object_type: u32) -> Option<Vec<(u32, FieldDef)>> {
    field_blocks(object_type).map(|b| with_bits(b).map(|(bit, d)| (bit, *d)).collect())
}

/// Bitmap words an instance of `object_type` stores: the word of its last field + 1.
pub fn bitmap_words(object_type: u32) -> Option<usize> {
    field_blocks(object_type).and_then(|b| with_bits(b).last()).map(|(bit, _)| bit as usize / 32 + 1)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectError {
    /// Ran out of data reading `what` at `offset`.
    Truncated { offset: usize, what: &'static str },
    BadVersion(u32),
    UnsupportedType(u32),
    /// A presence byte that is neither 0 nor 1.
    BadFlag { offset: usize, field: &'static str, value: u8 },
    /// Bytes left over after the last field.
    TrailingBytes { offset: usize, len: usize },
    BadBitmap { declared: u16, set: u32 },
    /// `set` bitmap bits are set but only `known` belong to fields of the type.
    UnknownBits { set: u32, known: u32 },
    BadString { offset: usize },
}

impl fmt::Display for ObjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { offset, what } => write!(f, "truncated reading {what} at offset {offset}"),
            Self::BadVersion(v) => write!(f, "unsupported object version {v:#x} (expected {VERSION:#x})"),
            Self::UnsupportedType(t) => write!(f, "object type {t} is not supported"),
            Self::BadFlag { offset, field, value } => write!(f, "{field}: presence byte {value} at offset {offset}"),
            Self::TrailingBytes { offset, len } => write!(f, "{} bytes left over after the last field (offset {offset})", len - offset),
            Self::BadBitmap { declared, set } => write!(f, "field count {declared} does not match {set} bits set"),
            Self::UnknownBits { set, known } => write!(f, "{set} bitmap bits set but only {known} name fields of the type"),
            Self::BadString { offset } => write!(f, "string at offset {offset} is not terminated"),
        }
    }
}

impl std::error::Error for ObjectError {}

/// How a field's value is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    /// 4 bytes.
    Int32,
    /// `u8 present`, then 8 bytes if present.
    Int64,
    /// Sizeable array: `u8 present`, then `u32 elem_size`, `u32 count`, `u32 id`, the elements,
    /// and a bitset (`u32 n` then `n` × u32).
    Array,
    /// `u8 present`, then a 24-byte [`ObjectId`] if present.
    ObjectRef,
    /// `u8 present`, then `u32 len`, `len` bytes and a NUL not counted in `len`.
    String,
}

/// One field of the engine's object field enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldDef {
    /// The engine's enum value (e.g. `pc_background` = 257).
    pub id: u32,
    /// The name without the `obj_f_` prefix.
    pub name: &'static str,
    pub storage: Storage,
}

/// A 24-byte object identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectId {
    /// -1 none, 1 prototype, 2 GUID (others exist for map objects).
    pub kind: i16,
    pub payload: [u8; 16],
}

impl ObjectId {
    pub const KIND_NONE: i16 = -1;
    pub const KIND_PROTOTYPE: i16 = 1;
    pub const KIND_GUID: i16 = 2;

    /// The prototype number, for prototype ids.
    pub fn prototype(&self) -> Option<u32> {
        (self.kind == Self::KIND_PROTOTYPE).then(|| u32::from_le_bytes(self.payload[..4].try_into().unwrap()))
    }

    /// The GUID as `XXXXXXXX_XXXX_XXXX_XXXX_XXXXXXXXXXXX`, the form used in file names.
    pub fn guid(&self) -> Option<String> {
        if self.kind != Self::KIND_GUID {
            return None;
        }
        let p = &self.payload;
        let d1 = u32::from_le_bytes(p[0..4].try_into().unwrap());
        let d2 = u16::from_le_bytes(p[4..6].try_into().unwrap());
        let d3 = u16::from_le_bytes(p[6..8].try_into().unwrap());
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02X}")).collect::<String>();
        Some(format!("{d1:08X}_{d2:04X}_{d3:04X}_{}_{}", hex(&p[8..10]), hex(&p[10..16])))
    }
}

/// A sizeable array's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Array {
    pub elem_size: u32,
    pub id: u32,
    /// `count * elem_size` bytes.
    pub data: Vec<u8>,
    pub bitset: Vec<u32>,
}

impl Array {
    pub fn len(&self) -> usize {
        if self.elem_size == 0 { 0 } else { self.data.len() / self.elem_size as usize }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The elements as i32s, for arrays of 4-byte elements.
    pub fn ints(&self) -> Option<Vec<i32>> {
        (self.elem_size == 4).then(|| self.data.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect())
    }
}

/// A field's value. `None` inside means stored as absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int32(i32),
    Int64(Option<i64>),
    Array(Option<Array>),
    ObjectRef(Option<ObjectId>),
    String(Option<String>),
}

/// A decoded object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    pub version: u32,
    pub prototype: ObjectId,
    pub id: ObjectId,
    pub object_type: u32,
    /// Prototypes: the "fields set" mask. Instances: which fields are stored.
    pub bitmap: Vec<u32>,
    pub fields: Vec<(FieldDef, Value)>,
}

impl Object {
    /// Parses a prototype (`proto/*.pro`). Only player characters so far: other types'
    /// prototype layouts are not verified and return [`ObjectError::UnsupportedType`].
    pub fn parse_prototype(data: &[u8]) -> Result<Self, ObjectError> {
        let mut r = Reader::new(data);
        let (version, prototype, id, object_type) = read_header(&mut r)?;
        if object_type != TYPE_PC {
            return Err(ObjectError::UnsupportedType(object_type));
        }
        let words = bitmap_words(TYPE_PC).unwrap();
        let bitmap = (0..words).map(|_| r.u32("bitmap")).collect::<Result<_, _>>()?;
        let mut fields = Vec::new();
        for (_, def) in with_bits(PC_BLOCKS) {
            fields.push((*def, read_value(&mut r, *def)?));
        }
        r.finish()?;
        Ok(Self { version, prototype, id, object_type, bitmap, fields })
    }

    /// Parses an instance object (a `.mob`, the body of a `.mpc`). `data` must be exactly the
    /// object.
    pub fn parse_instance(data: &[u8]) -> Result<Self, ObjectError> {
        let (object, used) = Self::parse_instance_prefix(data)?;
        if used != data.len() {
            return Err(ObjectError::TrailingBytes { offset: used, len: data.len() });
        }
        Ok(object)
    }

    /// Parses one instance object from the start of `data` and returns it with the number of
    /// bytes it took (objects in a sector list are stored back to back with no lengths).
    pub fn parse_instance_prefix(data: &[u8]) -> Result<(Self, usize), ObjectError> {
        let mut r = Reader::new(data);
        let (version, prototype, id, object_type) = read_header(&mut r)?;
        let blocks = field_blocks(object_type).ok_or(ObjectError::UnsupportedType(object_type))?;
        let words = bitmap_words(object_type).unwrap();
        let declared = r.u16("field count")?;
        let bitmap: Vec<u32> = (0..words).map(|_| r.u32("bitmap")).collect::<Result<_, _>>()?;
        let set: u32 = bitmap.iter().map(|w| w.count_ones()).sum();
        if set != u32::from(declared) {
            return Err(ObjectError::BadBitmap { declared, set });
        }
        let mut fields = Vec::with_capacity(set as usize);
        for (bit, def) in with_bits(blocks) {
            if bitmap[bit as usize / 32] >> (bit % 32) & 1 == 1 {
                fields.push((*def, read_value(&mut r, *def)?));
            }
        }
        if fields.len() as u32 != set {
            return Err(ObjectError::UnknownBits { set, known: fields.len() as u32 });
        }
        Ok((Self { version, prototype, id, object_type, bitmap, fields }, r.pos))
    }

    /// The current art id (`current_aid`), if stored.
    pub fn art_id(&self) -> Option<u32> {
        self.int("current_aid").map(|v| v as u32)
    }

    pub fn field(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find(|(d, _)| d.name == name).map(|(_, v)| v)
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        match self.field(name)? {
            Value::Int32(v) => Some(*v),
            _ => None,
        }
    }

    pub fn ints(&self, name: &str) -> Option<Vec<i32>> {
        match self.field(name)? {
            Value::Array(Some(a)) => a.ints(),
            Value::Array(None) => Some(Vec::new()),
            _ => None,
        }
    }

    pub fn string(&self, name: &str) -> Option<&str> {
        match self.field(name)? {
            Value::String(Some(s)) => Some(s),
            _ => None,
        }
    }
}

/// A pre-made character file (`Players/*.mpc`): an instance object plus two strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerFile {
    pub object: Object,
    /// The auto-level scheme ("Firearms 4, Melee 4, ...").
    pub level_scheme: String,
    /// The name stored with the file (two shipped files have placeholders such as "HOrcFemale";
    /// the prototype has the real name).
    pub name: String,
}

impl PlayerFile {
    pub fn parse(data: &[u8]) -> Result<Self, ObjectError> {
        let mut r = Reader::new(data);
        let len = r.u32("object length")? as usize;
        let body = r.bytes(len, "object")?;
        let object = Object::parse_instance(body)?;
        let level_scheme = r.counted_string("level scheme")?;
        let name = r.counted_string("name")?;
        r.finish()?;
        Ok(Self { object, level_scheme, name })
    }
}

fn read_header(r: &mut Reader) -> Result<(u32, ObjectId, ObjectId, u32), ObjectError> {
    let version = r.u32("version")?;
    if version != VERSION {
        return Err(ObjectError::BadVersion(version));
    }
    let prototype = r.object_id("prototype id")?;
    let id = r.object_id("object id")?;
    let object_type = r.u32("object type")?;
    Ok((version, prototype, id, object_type))
}

fn read_value(r: &mut Reader, def: FieldDef) -> Result<Value, ObjectError> {
    let name = def.name;
    Ok(match def.storage {
        Storage::Int32 => Value::Int32(r.u32(name)? as i32),
        Storage::Int64 => Value::Int64(if r.present(name)? { Some(r.u64(name)? as i64) } else { None }),
        Storage::ObjectRef => Value::ObjectRef(if r.present(name)? { Some(r.object_id(name)?) } else { None }),
        Storage::String => Value::String(if r.present(name)? {
            let len = r.u32(name)? as usize;
            let start = r.pos;
            let bytes = r.bytes(len, name)?;
            let text = bytes.iter().map(|&b| cp1252::decode_byte(b)).collect();
            if r.u8(name)? != 0 {
                return Err(ObjectError::BadString { offset: start });
            }
            Some(text)
        } else {
            None
        }),
        Storage::Array => Value::Array(if r.present(name)? {
            let elem_size = r.u32(name)?;
            let count = r.u32(name)? as usize;
            let id = r.u32(name)?;
            let size = count.checked_mul(elem_size as usize).ok_or(ObjectError::Truncated { offset: r.pos, what: name })?;
            let data = r.bytes(size, name)?.to_vec();
            let words = r.u32(name)? as usize;
            let bitset = (0..words).map(|_| r.u32(name)).collect::<Result<_, _>>()?;
            Some(Array { elem_size, id, data, bitset })
        } else {
            None
        }),
    })
}

/// A cursor over the bytes with offsets in its errors.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn bytes(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], ObjectError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or(ObjectError::Truncated { offset: self.pos, what })?;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u8(&mut self, what: &'static str) -> Result<u8, ObjectError> {
        Ok(self.bytes(1, what)?[0])
    }

    fn u16(&mut self, what: &'static str) -> Result<u16, ObjectError> {
        Ok(u16::from_le_bytes(self.bytes(2, what)?.try_into().unwrap()))
    }

    fn u32(&mut self, what: &'static str) -> Result<u32, ObjectError> {
        Ok(u32::from_le_bytes(self.bytes(4, what)?.try_into().unwrap()))
    }

    fn u64(&mut self, what: &'static str) -> Result<u64, ObjectError> {
        Ok(u64::from_le_bytes(self.bytes(8, what)?.try_into().unwrap()))
    }

    fn present(&mut self, field: &'static str) -> Result<bool, ObjectError> {
        let offset = self.pos;
        match self.u8(field)? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(ObjectError::BadFlag { offset, field, value }),
        }
    }

    fn object_id(&mut self, what: &'static str) -> Result<ObjectId, ObjectError> {
        let b = self.bytes(24, what)?;
        Ok(ObjectId { kind: i16::from_le_bytes([b[0], b[1]]), payload: b[8..24].try_into().unwrap() })
    }

    /// `u32 len` (including the NUL) then the text.
    fn counted_string(&mut self, what: &'static str) -> Result<String, ObjectError> {
        let start = self.pos;
        let len = self.u32(what)? as usize;
        let bytes = self.bytes(len, what)?;
        let (text, nul) = bytes.split_at(len.saturating_sub(1));
        if nul != [0] {
            return Err(ObjectError::BadString { offset: start });
        }
        Ok(text.iter().map(|&b| cp1252::decode_byte(b)).collect())
    }

    fn finish(&self) -> Result<(), ObjectError> {
        if self.pos == self.data.len() { Ok(()) } else { Err(ObjectError::TrailingBytes { offset: self.pos, len: self.data.len() }) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_defs() -> Vec<(u32, FieldDef)> {
        field_bits(TYPE_PC).unwrap()
    }

    fn header(out: &mut Vec<u8>, proto_kind: i16, proto: u32) {
        out.extend(VERSION.to_le_bytes());
        // Prototype id.
        out.extend(proto_kind.to_le_bytes());
        out.extend([0u8; 6]);
        let mut payload = [0u8; 16];
        payload[..4].copy_from_slice(&proto.to_le_bytes());
        out.extend(payload);
        // Own id: a GUID.
        out.extend(ObjectId::KIND_GUID.to_le_bytes());
        out.extend([0u8; 6]);
        out.extend([0xE3, 0xD5, 0x39, 0x2B, 0x96, 0x1C, 0x8F, 0x41, 0xA6, 0x34, 0x9E, 0xE5, 0x80, 0x53, 0xE3, 0xBF]);
        out.extend(TYPE_PC.to_le_bytes());
    }

    /// Writes `def`'s value: special values for a few fields, absent/zero for the rest.
    fn value(out: &mut Vec<u8>, def: &FieldDef) {
        match (def.name, def.storage) {
            ("pc_background", _) => out.extend(46i32.to_le_bytes()),
            ("critter_portrait", _) => out.extend(1020i32.to_le_bytes()),
            ("pc_player_name", _) => {
                out.push(1);
                out.extend(6u32.to_le_bytes());
                out.extend(b"Lugard");
                out.push(0);
            }
            ("critter_stat_base_idx", _) => {
                out.push(1);
                out.extend(4u32.to_le_bytes());
                out.extend(3u32.to_le_bytes());
                out.extend(0x2df2u32.to_le_bytes());
                for v in [8i32, 1, 1] {
                    out.extend(v.to_le_bytes());
                }
                out.extend(2u32.to_le_bytes());
                out.extend([7u32.to_le_bytes(), 0u32.to_le_bytes()].concat());
            }
            ("location", _) => {
                out.push(1);
                out.extend(0x0086_0000_077cu64.to_le_bytes());
            }
            (_, Storage::Int32) => out.extend(0i32.to_le_bytes()),
            _ => out.push(0),
        }
    }

    fn prototype_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        header(&mut out, ObjectId::KIND_NONE, 0);
        out.extend([0u8; 20]);
        for (_, def) in all_defs() {
            value(&mut out, &def);
        }
        out
    }

    fn instance_bytes(bits: &[u32]) -> Vec<u8> {
        let mut out = Vec::new();
        header(&mut out, ObjectId::KIND_PROTOTYPE, 16094);
        out.extend((bits.len() as u16).to_le_bytes());
        let mut words = [0u32; 5];
        for b in bits {
            words[*b as usize / 32] |= 1 << (b % 32);
        }
        for w in words {
            out.extend(w.to_le_bytes());
        }
        for (bit, def) in all_defs() {
            if bits.contains(&bit) {
                value(&mut out, &def);
            }
        }
        out
    }

    #[test]
    fn field_tables_follow_the_engine_enum() {
        assert_eq!((COMMON_FIELDS.len(), CRITTER_FIELDS.len(), PC_FIELDS.len()), (36, 33, 25));
        let bg = PC_FIELDS.iter().find(|d| d.name == "pc_background").unwrap();
        assert_eq!(bg.id, 0x101, "the id the picker code in Arcanum.exe reads");
        // Ids run consecutively within each block.
        for block in [&COMMON_FIELDS[..], &CRITTER_FIELDS[..], &PC_FIELDS[..]] {
            assert!(block.windows(2).all(|w| w[1].id == w[0].id + 1));
        }
        // Every block fits its 64-bit range of the bitmap.
        assert!(all_defs().iter().all(|(bit, _)| (*bit as usize) < 5 * 32));
    }

    #[test]
    fn prototypes_store_every_field() {
        let obj = Object::parse_prototype(&prototype_bytes()).unwrap();
        assert_eq!(obj.fields.len(), 36 + 33 + 25);
        assert_eq!(obj.prototype.kind, ObjectId::KIND_NONE);
        assert_eq!(obj.id.guid().as_deref(), Some("2B39D5E3_1C96_418F_A634_9EE58053E3BF"));
        assert_eq!(obj.int("pc_background"), Some(46));
        assert_eq!(obj.int("critter_portrait"), Some(1020));
        assert_eq!(obj.string("pc_player_name"), Some("Lugard"));
        assert_eq!(obj.ints("critter_stat_base_idx"), Some(vec![8, 1, 1]));
        assert_eq!(obj.field("location"), Some(&Value::Int64(Some(0x0086_0000_077c))));
        assert_eq!(obj.field("critter_gold"), Some(&Value::ObjectRef(None)));
        assert_eq!(obj.ints("pc_quest_idx"), Some(vec![]), "absent arrays read as empty");
    }

    #[test]
    fn instances_store_only_the_bitmap_fields() {
        // current_aid (0), location (1), stats (66), portrait (77), background (132), name (145).
        let bits = [0, 1, 66, 77, 132, 145];
        let obj = Object::parse_instance(&instance_bytes(&bits)).unwrap();
        assert_eq!(obj.fields.len(), bits.len());
        assert_eq!(obj.prototype.prototype(), Some(16094));
        assert_eq!(obj.int("pc_background"), Some(46));
        assert_eq!(obj.string("pc_player_name"), Some("Lugard"));
        assert_eq!(obj.field("pc_flags"), None, "not stored");
    }

    #[test]
    fn player_files_wrap_an_instance_and_two_strings() {
        let body = instance_bytes(&[0, 66]);
        let mut file = (body.len() as u32).to_le_bytes().to_vec();
        file.extend(&body);
        for s in ["Firearms 4", "Lugard Bloodstone"] {
            file.extend((s.len() as u32 + 1).to_le_bytes());
            file.extend(s.as_bytes());
            file.push(0);
        }
        let p = PlayerFile::parse(&file).unwrap();
        assert_eq!((p.level_scheme.as_str(), p.name.as_str()), ("Firearms 4", "Lugard Bloodstone"));
        assert_eq!(p.object.ints("critter_stat_base_idx"), Some(vec![8, 1, 1]));
    }

    use crate::test_util::{ObjectBuilder, object_id_bytes, prototype_id};

    #[test]
    fn type_numbers_follow_the_engine_enum() {
        let types = [
            TYPE_WALL, TYPE_PORTAL, TYPE_CONTAINER, TYPE_SCENERY, TYPE_PROJECTILE, TYPE_WEAPON, TYPE_AMMO, TYPE_ARMOR, TYPE_GOLD,
            TYPE_FOOD, TYPE_SCROLL, TYPE_KEY, TYPE_KEY_RING, TYPE_WRITTEN, TYPE_GENERIC, TYPE_PC, TYPE_NPC, TYPE_TRAP,
        ];
        assert_eq!(types.to_vec(), (0..18).collect::<Vec<u32>>());
    }

    #[test]
    fn bitmap_words_per_type() {
        let words: Vec<_> = (0..18).map(|t| bitmap_words(t).unwrap()).collect();
        assert_eq!(words, [3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 3]);
        assert_eq!(bitmap_words(18), None);
    }

    /// Spot checks against research-object-fields.tsv (the engine's table walked per type).
    #[test]
    fn field_bits_match_the_engine_tables() {
        let bit = |t: u32, name: &str| field_bits(t).unwrap().into_iter().find(|(_, d)| d.name == name).map(|(b, d)| (b, d.id, d.storage));
        assert_eq!(bit(TYPE_WALL, "current_aid"), Some((0, 1, Storage::Int32)));
        assert_eq!(bit(TYPE_WALL, "wall_flags"), Some((64, 39, Storage::Int32)));
        assert_eq!(bit(TYPE_WALL, "wall_pad_i64as_1"), Some((68, 43, Storage::Array)));
        assert_eq!(bit(TYPE_PORTAL, "portal_flags"), Some((64, 46, Storage::Int32)));
        assert_eq!(bit(TYPE_CONTAINER, "container_flags"), Some((64, 56, Storage::Int32)));
        assert_eq!(bit(TYPE_SCENERY, "scenery_whos_in_me"), Some((65, 70, Storage::ObjectRef)));
        assert_eq!(bit(TYPE_PROJECTILE, "projectile_parent_weapon"), Some((67, 80, Storage::ObjectRef)));
        assert_eq!(bit(TYPE_WEAPON, "item_parent"), Some((65, 88, Storage::ObjectRef)));
        assert_eq!(bit(TYPE_WEAPON, "weapon_flags"), Some((96, 112, Storage::Int32)));
        assert_eq!(bit(TYPE_WEAPON, "weapon_pad_i64as_1"), Some((122, 138, Storage::Array)));
        assert_eq!(bit(TYPE_AMMO, "ammo_flags"), Some((96, 141, Storage::Int32)));
        assert_eq!(bit(TYPE_KEY_RING, "key_ring_flags"), Some((96, 193, Storage::Int32)));
        assert_eq!(bit(TYPE_GENERIC, "generic_flags"), Some((96, 211, Storage::Int32)));
        assert_eq!(bit(TYPE_NPC, "critter_flags"), Some((64, 218, Storage::Int32)));
        assert_eq!(bit(TYPE_NPC, "npc_flags"), Some((128, 280, Storage::Int32)));
        assert_eq!(bit(TYPE_NPC, "npc_standpoint_day"), Some((137, 289, Storage::Int64)));
        assert_eq!(bit(TYPE_PC, "pc_player_name"), Some((145, 270, Storage::String)));
        assert_eq!(bit(TYPE_TRAP, "trap_flags"), Some((64, 307, Storage::Int32)));
        assert_eq!(bit(TYPE_TRAP, "trap_pad_i64as_1"), Some((68, 311, Storage::Array)));
        // Blocks of other types are not part of a type.
        assert_eq!(bit(TYPE_WALL, "npc_flags"), None);
        // Per type: common + its blocks; 1123 stored fields over the 18 types.
        let total: usize = (0..18).map(|t| field_bits(t).unwrap().len()).sum();
        assert_eq!(total, 1123);
    }

    #[test]
    fn wall_instance_round_trip() {
        // Written by hand: current_aid (bit 0), location (bit 1), wall_flags (bit 64).
        let mut b = VERSION.to_le_bytes().to_vec();
        b.extend(object_id_bytes(&prototype_id(1012)));
        b.extend([0u8; 24]);
        b.extend(TYPE_WALL.to_le_bytes());
        b.extend(3u16.to_le_bytes());
        for w in [0b11u32, 0, 1] {
            b.extend(w.to_le_bytes());
        }
        b.extend(0x2000_1234u32.to_le_bytes());
        b.push(1);
        b.extend(((5i64 << 32) | 7).to_le_bytes());
        b.extend(0x40u32.to_le_bytes());
        let obj = Object::parse_instance(&b).unwrap();
        assert_eq!(obj.object_type, TYPE_WALL);
        assert_eq!(obj.prototype.prototype(), Some(1012));
        assert_eq!(obj.bitmap, vec![3, 0, 1]);
        assert_eq!(obj.art_id(), Some(0x2000_1234));
        assert_eq!(obj.field("location"), Some(&Value::Int64(Some((5 << 32) | 7))));
        assert_eq!(obj.int("wall_flags"), Some(0x40));
        assert_eq!(obj.fields.len(), 3);
        // The builder writes the same bytes.
        let built = ObjectBuilder { prototype: prototype_id(1012), ..ObjectBuilder::new(TYPE_WALL) }
            .field("wall_flags", Value::Int32(0x40))
            .field("current_aid", Value::Int32(0x2000_1234))
            .field("location", Value::Int64(Some((5 << 32) | 7)))
            .build();
        assert_eq!(built, b);
    }

    /// A sample value of each storage kind.
    fn sample(storage: Storage, seed: i32) -> Value {
        match storage {
            Storage::Int32 => Value::Int32(seed),
            Storage::Int64 => Value::Int64(Some(i64::from(seed) << 33)),
            Storage::ObjectRef => Value::ObjectRef(Some(prototype_id(seed as u32))),
            Storage::String => Value::String(Some(format!("s{seed}"))),
            Storage::Array => Value::Array(Some(Array { elem_size: 24, id: seed as u32, data: vec![seed as u8; 48], bitset: vec![3] })),
        }
    }

    /// Sets the first and last field of every block, plus one field of each storage kind,
    /// and checks they come back in bit order with their values.
    fn round_trip(object_type: u32) {
        let bits = field_bits(object_type).unwrap();
        let mut chosen: Vec<(u32, FieldDef)> = Vec::new();
        for block in field_blocks(object_type).unwrap() {
            for def in [block[0], block[block.len() - 1]] {
                chosen.push(*bits.iter().find(|(_, d)| d.id == def.id).unwrap());
            }
        }
        for kind in [Storage::Int32, Storage::Int64, Storage::ObjectRef, Storage::String, Storage::Array] {
            if let Some(f) = bits.iter().find(|(_, d)| d.storage == kind) {
                chosen.push(*f);
            }
        }
        chosen.sort_by_key(|(b, _)| *b);
        chosen.dedup_by_key(|(b, _)| *b);
        let mut builder = ObjectBuilder::new(object_type);
        for (bit, def) in &chosen {
            builder = builder.field(def.name, sample(def.storage, *bit as i32 + 1));
        }
        let obj = Object::parse_instance(&builder.build()).unwrap_or_else(|e| panic!("type {object_type}: {e}"));
        assert_eq!(obj.bitmap.len(), bitmap_words(object_type).unwrap());
        let got: Vec<_> = obj.fields.iter().map(|(d, v)| (d.name, v.clone())).collect();
        let want: Vec<_> = chosen.iter().map(|(bit, d)| (d.name, sample(d.storage, *bit as i32 + 1))).collect();
        assert_eq!(got, want, "type {object_type}");
    }

    #[test]
    fn every_type_round_trips() {
        for t in 0..18 {
            round_trip(t);
        }
    }

    /// Projectiles never occur in the shipped sector or `.mob` data.
    #[test]
    fn projectile_instance_round_trip() {
        round_trip(TYPE_PROJECTILE);
        let obj = Object::parse_instance(
            &ObjectBuilder::new(TYPE_PROJECTILE)
                .field("projectile_parent_weapon", Value::ObjectRef(Some(prototype_id(4000))))
                .field("projectile_flags_combat", Value::Int32(9))
                .build(),
        )
        .unwrap();
        assert_eq!(obj.field("projectile_parent_weapon"), Some(&Value::ObjectRef(Some(prototype_id(4000)))));
        assert_eq!(obj.int("projectile_flags_combat"), Some(9));
    }

    #[test]
    fn parse_prefix_reports_consumed_bytes() {
        let first = ObjectBuilder::new(TYPE_SCENERY).field("current_aid", Value::Int32(77)).field("scenery_flags", Value::Int32(1)).build();
        let second = ObjectBuilder::new(TYPE_TRAP).field("trap_flags", Value::Int32(2)).build();
        let both = [first.clone(), second.clone()].concat();
        let (a, used) = Object::parse_instance_prefix(&both).unwrap();
        assert_eq!(used, first.len());
        assert_eq!(a.art_id(), Some(77));
        let (b, used2) = Object::parse_instance_prefix(&both[used..]).unwrap();
        assert_eq!((b.object_type, used2), (TYPE_TRAP, second.len()));
        assert_eq!(b.art_id(), None);
        // parse_instance insists on the exact length.
        assert_eq!(Object::parse_instance(&both), Err(ObjectError::TrailingBytes { offset: first.len(), len: both.len() }));
    }

    #[test]
    fn unknown_type_is_rejected() {
        let mut b = ObjectBuilder::new(TYPE_WALL).build();
        b[52..56].copy_from_slice(&99u32.to_le_bytes());
        assert_eq!(Object::parse_instance(&b), Err(ObjectError::UnsupportedType(99)));
        assert_eq!(Object::parse_instance_prefix(&b).map(|(_, n)| n), Err(ObjectError::UnsupportedType(99)));
    }

    #[test]
    fn bits_past_the_last_field_are_rejected() {
        // Wall fields end at bit 68; bit 69 has no field.
        let b = ObjectBuilder { extra_bits: vec![69], ..ObjectBuilder::new(TYPE_WALL) }.field("wall_flags", Value::Int32(1)).build();
        assert_eq!(Object::parse_instance(&b), Err(ObjectError::UnknownBits { set: 2, known: 1 }));
    }

    #[test]
    fn errors_are_reported_not_panics() {
        let good = prototype_bytes();
        // Every truncation fails cleanly.
        for len in 0..good.len() {
            assert!(Object::parse_prototype(&good[..len]).is_err(), "{len}");
        }
        let mut extra = good.clone();
        extra.push(0);
        assert!(matches!(Object::parse_prototype(&extra), Err(ObjectError::TrailingBytes { .. })));
        let mut bad_version = good.clone();
        bad_version[0] = 0x76;
        assert_eq!(Object::parse_prototype(&bad_version), Err(ObjectError::BadVersion(0x76)));
        let mut npc = good.clone();
        npc[52] = 16;
        assert_eq!(Object::parse_prototype(&npc), Err(ObjectError::UnsupportedType(16)));
        // A presence byte that is not 0 or 1.
        let mut bad_flag = good.clone();
        let location = 76 + 4;
        bad_flag[location] = 7;
        assert!(matches!(Object::parse_prototype(&bad_flag), Err(ObjectError::BadFlag { field: "location", .. })));
        // A field count that disagrees with the bitmap.
        let mut inst = instance_bytes(&[0, 66]);
        inst[56] = 3;
        assert_eq!(Object::parse_instance(&inst), Err(ObjectError::BadBitmap { declared: 3, set: 2 }));
    }
}
