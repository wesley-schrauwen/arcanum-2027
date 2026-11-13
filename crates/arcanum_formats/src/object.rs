//! Arcanum's binary object format: prototypes (`proto/*.pro`), pre-made characters
//! (`Players/*.mpc`) and, later, map and save objects.
//!
//! Every object is a header plus a list of fields. The field list per object type and its order
//! come from the engine's own `obj_f_*` enum (the names are in Arcanum.exe). Each field has one
//! of five storage kinds; see [`Storage`]. Two layouts exist:
//! - **Prototype**: header, a 20-byte "fields set" mask, then *every* field of the type in order.
//! - **Instance**: header, a field count and a bitmap, then only the fields whose bits are set.
//!
//! Only player characters (type 15) are supported so far. See NOTES.md ("Object format") for the
//! evidence behind each rule.

use std::fmt;

use crate::cp1252;

/// Format version written by the shipped game.
pub const VERSION: u32 = 0x77;
/// Object type of a player character.
pub const TYPE_PC: u32 = 15;
/// Bitmap words for a player character: three blocks of 64 bits would need 6, the game writes 5.
const PC_BITMAP_WORDS: usize = 5;
/// Bitmap bit where each block starts.
const COMMON_BIT: u32 = 0;
const CRITTER_BIT: u32 = 64;
const PC_BIT: u32 = 128;

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
    BadString { offset: usize },
}

impl fmt::Display for ObjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { offset, what } => write!(f, "truncated reading {what} at offset {offset}"),
            Self::BadVersion(v) => write!(f, "unsupported object version {v:#x} (expected {VERSION:#x})"),
            Self::UnsupportedType(t) => write!(f, "object type {t} is not supported yet"),
            Self::BadFlag { offset, field, value } => write!(f, "{field}: presence byte {value} at offset {offset}"),
            Self::TrailingBytes { offset, len } => write!(f, "{} bytes left over after the last field (offset {offset})", len - offset),
            Self::BadBitmap { declared, set } => write!(f, "field count {declared} does not match {set} bits set"),
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
    /// Parses a prototype (`proto/*.pro`).
    pub fn parse_prototype(data: &[u8]) -> Result<Self, ObjectError> {
        let mut r = Reader::new(data);
        let (version, prototype, id, object_type) = read_header(&mut r)?;
        let defs = fields_for(object_type)?;
        let bitmap = (0..PC_BITMAP_WORDS).map(|_| r.u32("bitmap")).collect::<Result<_, _>>()?;
        let mut fields = Vec::with_capacity(defs.len());
        for (_, def) in defs {
            fields.push((def, read_value(&mut r, def)?));
        }
        r.finish()?;
        Ok(Self { version, prototype, id, object_type, bitmap, fields })
    }

    /// Parses an instance object (for example the body of a `.mpc`). `data` must be exactly the
    /// object.
    pub fn parse_instance(data: &[u8]) -> Result<Self, ObjectError> {
        let mut r = Reader::new(data);
        let (version, prototype, id, object_type) = read_header(&mut r)?;
        let defs = fields_for(object_type)?;
        let declared = r.u16("field count")?;
        let bitmap: Vec<u32> = (0..PC_BITMAP_WORDS).map(|_| r.u32("bitmap")).collect::<Result<_, _>>()?;
        let set: u32 = bitmap.iter().map(|w| w.count_ones()).sum();
        if set != u32::from(declared) {
            return Err(ObjectError::BadBitmap { declared, set });
        }
        let is_set = |bit: u32| bitmap.get(bit as usize / 32).is_some_and(|w| w >> (bit % 32) & 1 == 1);
        let mut fields = Vec::new();
        for (bit, def) in defs {
            if is_set(bit) {
                fields.push((def, read_value(&mut r, def)?));
            }
        }
        r.finish()?;
        Ok(Self { version, prototype, id, object_type, bitmap, fields })
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

/// The fields of `object_type`, each with its bitmap bit.
fn fields_for(object_type: u32) -> Result<Vec<(u32, FieldDef)>, ObjectError> {
    if object_type != TYPE_PC {
        return Err(ObjectError::UnsupportedType(object_type));
    }
    let block = |start: u32, defs: &[FieldDef]| defs.iter().enumerate().map(move |(i, d)| (start + i as u32, *d)).collect::<Vec<_>>();
    let mut all = block(COMMON_BIT, &COMMON_FIELDS);
    all.extend(block(CRITTER_BIT, &CRITTER_FIELDS));
    all.extend(block(PC_BIT, &PC_FIELDS));
    Ok(all)
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

// Field tables, generated from the engine's `obj_f_*` names (tools/object_probe). Storage kinds
// follow the names (`*_idx`, `*ias_*` and overlays are arrays; location and teleport destination
// are 64-bit; fleeing-from, gold and ammo are object references; the player name is a string),
// confirmed by parsing every shipped PC prototype and pre-made character file exactly.

/// Fields every object has (bitmap bits 0..).
pub const COMMON_FIELDS: [FieldDef; 36] = [
    FieldDef { id: 1, name: "current_aid", storage: Storage::Int32 },
    FieldDef { id: 2, name: "location", storage: Storage::Int64 },
    FieldDef { id: 3, name: "offset_x", storage: Storage::Int32 },
    FieldDef { id: 4, name: "offset_y", storage: Storage::Int32 },
    FieldDef { id: 5, name: "shadow", storage: Storage::Int32 },
    FieldDef { id: 6, name: "overlay_fore", storage: Storage::Array },
    FieldDef { id: 7, name: "overlay_back", storage: Storage::Array },
    FieldDef { id: 8, name: "underlay", storage: Storage::Array },
    FieldDef { id: 9, name: "blit_flags", storage: Storage::Int32 },
    FieldDef { id: 10, name: "blit_color", storage: Storage::Int32 },
    FieldDef { id: 11, name: "blit_alpha", storage: Storage::Int32 },
    FieldDef { id: 12, name: "blit_scale", storage: Storage::Int32 },
    FieldDef { id: 13, name: "light_flags", storage: Storage::Int32 },
    FieldDef { id: 14, name: "light_aid", storage: Storage::Int32 },
    FieldDef { id: 15, name: "light_color", storage: Storage::Int32 },
    FieldDef { id: 16, name: "overlay_light_flags", storage: Storage::Array },
    FieldDef { id: 17, name: "overlay_light_aid", storage: Storage::Array },
    FieldDef { id: 18, name: "overlay_light_color", storage: Storage::Array },
    FieldDef { id: 19, name: "flags", storage: Storage::Int32 },
    FieldDef { id: 20, name: "spell_flags", storage: Storage::Int32 },
    FieldDef { id: 21, name: "blocking_mask", storage: Storage::Int32 },
    FieldDef { id: 22, name: "name", storage: Storage::Int32 },
    FieldDef { id: 23, name: "description", storage: Storage::Int32 },
    FieldDef { id: 24, name: "aid", storage: Storage::Int32 },
    FieldDef { id: 25, name: "destroyed_aid", storage: Storage::Int32 },
    FieldDef { id: 26, name: "AC", storage: Storage::Int32 },
    FieldDef { id: 27, name: "hp_pts", storage: Storage::Int32 },
    FieldDef { id: 28, name: "hp_adj", storage: Storage::Int32 },
    FieldDef { id: 29, name: "hp_damage", storage: Storage::Int32 },
    FieldDef { id: 30, name: "material", storage: Storage::Int32 },
    FieldDef { id: 31, name: "resistance_idx", storage: Storage::Array },
    FieldDef { id: 32, name: "scripts_idx", storage: Storage::Array },
    FieldDef { id: 33, name: "sound_effect", storage: Storage::Int32 },
    FieldDef { id: 34, name: "category", storage: Storage::Int32 },
    FieldDef { id: 35, name: "pad_ias_1", storage: Storage::Array },
    FieldDef { id: 36, name: "pad_i64as_1", storage: Storage::Array },
];

/// Critter fields (bitmap bits 64..).
pub const CRITTER_FIELDS: [FieldDef; 33] = [
    FieldDef { id: 218, name: "critter_flags", storage: Storage::Int32 },
    FieldDef { id: 219, name: "critter_flags2", storage: Storage::Int32 },
    FieldDef { id: 220, name: "critter_stat_base_idx", storage: Storage::Array },
    FieldDef { id: 221, name: "critter_basic_skill_idx", storage: Storage::Array },
    FieldDef { id: 222, name: "critter_tech_skill_idx", storage: Storage::Array },
    FieldDef { id: 223, name: "critter_spell_tech_idx", storage: Storage::Array },
    FieldDef { id: 224, name: "critter_fatigue_pts", storage: Storage::Int32 },
    FieldDef { id: 225, name: "critter_fatigue_adj", storage: Storage::Int32 },
    FieldDef { id: 226, name: "critter_fatigue_damage", storage: Storage::Int32 },
    FieldDef { id: 227, name: "critter_crit_hit_chart", storage: Storage::Int32 },
    FieldDef { id: 228, name: "critter_effects_idx", storage: Storage::Array },
    FieldDef { id: 229, name: "critter_effect_cause_idx", storage: Storage::Array },
    FieldDef { id: 230, name: "critter_fleeing_from", storage: Storage::ObjectRef },
    FieldDef { id: 231, name: "critter_portrait", storage: Storage::Int32 },
    FieldDef { id: 232, name: "critter_gold", storage: Storage::ObjectRef },
    FieldDef { id: 233, name: "critter_arrows", storage: Storage::ObjectRef },
    FieldDef { id: 234, name: "critter_bullets", storage: Storage::ObjectRef },
    FieldDef { id: 235, name: "critter_power_cells", storage: Storage::ObjectRef },
    FieldDef { id: 236, name: "critter_fuel", storage: Storage::ObjectRef },
    FieldDef { id: 237, name: "critter_inventory_num", storage: Storage::Int32 },
    FieldDef { id: 238, name: "critter_inventory_list_idx", storage: Storage::Array },
    FieldDef { id: 239, name: "critter_inventory_source", storage: Storage::Int32 },
    FieldDef { id: 240, name: "critter_description_unknown", storage: Storage::Int32 },
    FieldDef { id: 241, name: "critter_follower_idx", storage: Storage::Array },
    FieldDef { id: 242, name: "critter_teleport_dest", storage: Storage::Int64 },
    FieldDef { id: 243, name: "critter_teleport_map", storage: Storage::Int32 },
    FieldDef { id: 244, name: "critter_death_time", storage: Storage::Int32 },
    FieldDef { id: 245, name: "critter_auto_level_scheme", storage: Storage::Int32 },
    FieldDef { id: 246, name: "critter_pad_i_1", storage: Storage::Int32 },
    FieldDef { id: 247, name: "critter_pad_i_2", storage: Storage::Int32 },
    FieldDef { id: 248, name: "critter_pad_i_3", storage: Storage::Int32 },
    FieldDef { id: 249, name: "critter_pad_ias_1", storage: Storage::Array },
    FieldDef { id: 250, name: "critter_pad_i64as_1", storage: Storage::Array },
];

/// Player-character fields (bitmap bits 128..).
pub const PC_FIELDS: [FieldDef; 25] = [
    FieldDef { id: 253, name: "pc_flags", storage: Storage::Int32 },
    FieldDef { id: 254, name: "pc_flags_fate", storage: Storage::Int32 },
    FieldDef { id: 255, name: "pc_reputation_idx", storage: Storage::Array },
    FieldDef { id: 256, name: "pc_reputation_ts_idx", storage: Storage::Array },
    FieldDef { id: 257, name: "pc_background", storage: Storage::Int32 },
    FieldDef { id: 258, name: "pc_background_text", storage: Storage::Int32 },
    FieldDef { id: 259, name: "pc_quest_idx", storage: Storage::Array },
    FieldDef { id: 260, name: "pc_blessing_idx", storage: Storage::Array },
    FieldDef { id: 261, name: "pc_blessing_ts_idx", storage: Storage::Array },
    FieldDef { id: 262, name: "pc_curse_idx", storage: Storage::Array },
    FieldDef { id: 263, name: "pc_curse_ts_idx", storage: Storage::Array },
    FieldDef { id: 264, name: "pc_party_id", storage: Storage::Int32 },
    FieldDef { id: 265, name: "pc_rumor_idx", storage: Storage::Array },
    FieldDef { id: 266, name: "pc_pad_ias_2", storage: Storage::Array },
    FieldDef { id: 267, name: "pc_schematics_found_idx", storage: Storage::Array },
    FieldDef { id: 268, name: "pc_logbook_ego_idx", storage: Storage::Array },
    FieldDef { id: 269, name: "pc_fog_mask", storage: Storage::Int32 },
    FieldDef { id: 270, name: "pc_player_name", storage: Storage::String },
    FieldDef { id: 271, name: "pc_bank_money", storage: Storage::Int32 },
    FieldDef { id: 272, name: "pc_global_flags", storage: Storage::Array },
    FieldDef { id: 273, name: "pc_global_variables", storage: Storage::Array },
    FieldDef { id: 274, name: "pc_pad_i_1", storage: Storage::Int32 },
    FieldDef { id: 275, name: "pc_pad_i_2", storage: Storage::Int32 },
    FieldDef { id: 276, name: "pc_pad_ias_1", storage: Storage::Array },
    FieldDef { id: 277, name: "pc_pad_i64as_1", storage: Storage::Array },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn all_defs() -> Vec<(u32, FieldDef)> {
        fields_for(TYPE_PC).unwrap()
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
        let mut words = [0u32; PC_BITMAP_WORDS];
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
        assert!(all_defs().iter().all(|(bit, _)| (*bit as usize) < PC_BITMAP_WORDS * 32));
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
