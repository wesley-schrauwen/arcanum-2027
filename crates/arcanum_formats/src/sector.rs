//! Sector files (`maps/<map>/<sector id>.sec`): one 64 x 64 tile square of a map.
//!
//! Loader: the function at 0x4d1a40 in Arcanum.exe (called from the sector cache, `FUN_004d0540`
//! at 0x4d0540); file name `"%s\%I64u.sec"` (0x5b7e6c, built by `FUN_004d04a0`). Blocks in file
//! order, little endian, verified on all 1680 `.sec` in `Arcanum.dat` (every one parses to the
//! exact last byte):
//!
//! | Size | Block | Engine reader |
//! |---|---|---|
//! | 4 | `u32 light_count` | 0x4f72c0 |
//! | 48 * n | lights ([`Light`]) | 0x4ddd20 |
//! | 16384 | 4096 x `u32` tile art id ([`Tile`]), row-major | 0x4f7c30 / 0x4f7ce0 |
//! | 4 | `u32 roof_empty_flag` | 0x4f7f80 |
//! | 1024 / 0 | if the flag is 0: 256 x `u32` roof art ids (16 x 16, one per 4 x 4 tiles); else nothing | 0x4f8020 |
//! | 4 | `u32 version`, 0x00AA0000 + 0..=4 (the engine calls it "placeholder") | 0x4d1eb1 |
//! | 4 + 24 * n | version >= 1: `u32 count`, then [`TileScript`]s | 0x4f65b0 |
//! | 12 | version >= 2: sector script [`ScriptHeader`] | 0x4f6250 |
//! | 4, 4, 4 | version >= 3: town map info, aptitude adjustment (`i32`), light scheme | 0x4d1fd7 |
//! | 12 | version >= 3: sound list, 3 x `u32` | 0x4f8150 |
//! | 512 | version >= 4: tile blocking mask, 128 x `u32` | 0x4f8250 |
//! | ... | objects back to back, no lengths ([`Object::parse_instance_prefix`]) | 0x4f1350 |
//! | 4 | `u32 object_count` | 0x4f1350 |
//!
//! There is no count before the objects: the engine (`FUN_004f1350`) seeks to EOF-4, reads the
//! count, seeks back, reads that many objects, then reads a `u32` that must equal the count. Each
//! object has to be walked field by field to find the next one. Sector objects are static types
//! only (walls, portals, scenery, traps); containers, items and critters are `.mob` files.
//!
//! Unverified: the bit order of the blocking mask (read here as bit `i % 32` of word `i / 32` for
//! tile `i`, LSB first); the meanings of the light record's fields, the sound list and the three
//! tile flag bits (medium to low confidence, see NOTES.md).

use std::fmt;

use crate::art_id::ArtId;
use crate::map::SECTOR_TILES;
use crate::object::{Object, ObjectError};

/// Tiles in a sector.
pub const TILE_COUNT: usize = (SECTOR_TILES * SECTOR_TILES) as usize;
/// Roof art ids in a sector (one per 4 x 4 tiles).
pub const ROOF_COUNT: usize = TILE_COUNT / 16;
/// Words in the tile blocking mask.
pub const BLOCKED_WORDS: usize = TILE_COUNT / 32;
/// The version word is this plus 0..=[`MAX_VERSION`].
pub const VERSION_BASE: u32 = 0x00AA_0000;
pub const MAX_VERSION: u32 = 4;
/// Bytes per light record.
pub const LIGHT_SIZE: usize = 48;
/// Bytes per tile script record.
pub const TILE_SCRIPT_SIZE: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectorError {
    /// Ran out of data reading `what` at `offset`.
    Truncated { offset: usize, what: &'static str },
    /// A version word outside `0x00AA0000..=0x00AA0004`.
    BadVersion { offset: usize, value: u32 },
    /// Object `index` (starting at `offset`) failed to parse.
    Object { index: usize, offset: usize, error: ObjectError },
    /// The `u32` after the objects is not the object count at EOF-4.
    CountMismatch { declared: u32, after_objects: u32 },
    /// Bytes left over after the trailing object count; `total` is the whole file's length.
    TrailingBytes { offset: usize, total: usize },
}

impl fmt::Display for SectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { offset, what } => write!(f, "truncated reading {what} at offset {offset}"),
            Self::BadVersion { offset, value } => write!(f, "bad sector version {value:#x} at offset {offset}"),
            Self::Object { index, offset, error } => write!(f, "object {index} at offset {offset}: {error}"),
            Self::CountMismatch { declared, after_objects } => {
                write!(f, "object count {declared} at the end of the file, but {after_objects} after the objects")
            }
            Self::TrailingBytes { offset, total } => write!(f, "{} bytes left over at offset {offset} (file is {total} bytes)", total - offset),
        }
    }
}

impl std::error::Error for SectorError {}

/// A tile's art id (TIG art type in bits 28..31). Sector tiles are type 0 (tile) or 0xB (facade).
/// The accessors below decode type 0 (accessors 0x5036b0..0x503990 in Arcanum.exe) and are
/// meaningless for facade tiles ([`Tile::is_facade`]), whose ids use other bits (see `art_id`).
/// Bits 6..8 are the outdoor flag (bit 8) and the name-1 / name-2 flippable flags (bits 7 and 6),
/// as `art_id` uses them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Tile(pub u32);

impl Tile {
    pub const ART_TYPE_TILE: u32 = 0;
    pub const ART_TYPE_FACADE: u32 = 0xB;

    /// Bits 28..31 (0x502700).
    pub fn art_type(self) -> u32 {
        self.0 >> 28
    }

    /// This tile as an [`ArtId`], for [`ArtIndex::path`](crate::art_id::ArtIndex::path).
    pub fn art_id(self) -> ArtId {
        ArtId(self.0)
    }

    pub fn is_facade(self) -> bool {
        self.art_type() == Self::ART_TYPE_FACADE
    }

    /// Type 0 tiles only. First terrain: index into `art/tile/tilename.mes`, bits 22..27 (0x5036b0).
    pub fn terrain_1(self) -> u32 {
        (self.0 >> 22) & 0x3f
    }

    /// Type 0 tiles only. Second terrain (equal to the first on plain tiles), bits 16..21 (0x5036e0).
    pub fn terrain_2(self) -> u32 {
        (self.0 >> 16) & 0x3f
    }

    /// Type 0 tiles only. Edge/corner mask 0..15, bits 12..15 (0x503700): 0 = only terrain 2, 15 = only terrain 1.
    pub fn edge(self) -> u32 {
        (self.0 >> 12) & 0xf
    }

    /// Type 0 tiles only. Variation 0..7, bits 9..11 (0x5037b0).
    pub fn variation(self) -> u32 {
        (self.0 >> 9) & 0x7
    }

    /// Type 0 tiles only. Flag bits 6..8 as a 3-bit value (tested by 0x503990, 0x503950, 0x5038c0):
    /// bit 2 = outdoor (bit 8 of the id), bit 1 = name-1 flippable (bit 7), bit 0 = name-2
    /// flippable (bit 6), the meanings `art_id` uses for the path.
    pub fn flags(self) -> u32 {
        (self.0 >> 6) & 0x7
    }

    /// Type 0 tiles only. Bit 0, the flip bit (0x504fd0). Set in many transition tiles; the file name ignores it (see `art_id`).
    pub fn is_flipped(self) -> bool {
        self.0 & 1 == 1
    }
}

/// A 48-byte light record. Field names are medium confidence (`FUN_004de390`); runtime fields
/// (packed colour, handle) are not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Light {
    /// +0x00, 0 on disk in the samples.
    pub handle: u64,
    /// +0x08, absolute tile coordinates.
    pub x: i32,
    pub y: i32,
    /// +0x10, pixel offsets.
    pub offset_x: i32,
    pub offset_y: i32,
    /// +0x18.
    pub flags: u32,
    /// +0x1c, a light art id (art type 9).
    pub art_id: ArtId,
    /// +0x20, red, green, blue.
    pub color: [u8; 3],
}

/// A script attachment: flags, counters and the script number (0 = none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScriptHeader {
    pub flags: u32,
    pub counters: u32,
    pub number: u32,
}

/// A script on one tile (24 bytes; the last `u32` is a runtime pointer and is not kept).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TileScript {
    pub node_flags: u32,
    /// Tile index in the sector, 0..4096.
    pub tile: u32,
    pub script: ScriptHeader,
}

/// A parsed sector. Blocks a file's version does not have are zero (or `None`).
///
/// Blocking: [`Sector::blocked`] is only the sector's tile blocking mask. It is sparse (172 of
/// the 621 start-map sectors set any bit) and object blocking (walls, scenery, closed portals and
/// their prototype flags) is separate and not decoded yet, so a pathfinder cannot use the mask
/// alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sector {
    pub lights: Vec<Light>,
    /// Row-major: tile `(x, y)` is at `y * 64 + x`.
    pub tiles: Box<[Tile; TILE_COUNT]>,
    /// 16 x 16 roof art ids, row-major, [`ArtId::NONE`] for an empty cell. `None` when the file
    /// stores the "no roofs" flag (the engine fills every roof with -1).
    pub roofs: Option<Box<[ArtId; ROOF_COUNT]>>,
    /// 0..=4 (the low byte of the version word).
    pub version: u32,
    /// Version >= 1.
    pub tile_scripts: Vec<TileScript>,
    /// Version >= 2.
    pub script: ScriptHeader,
    /// Version >= 3.
    pub townmap_info: u32,
    pub aptitude_adjustment: i32,
    pub light_scheme: u32,
    pub sound_list: [u32; 3],
    /// Version >= 4: 4096-bit tile blocking mask, bit `i % 32` of word `i / 32` = tile `i` (bit
    /// order unverified). `None` for older versions (no mask in the file, which is not the same
    /// as nothing blocked). Tiles only: object blocking is not included.
    pub blocked: Option<Box<[u32; BLOCKED_WORDS]>>,
    /// Walls, portals, scenery and traps.
    pub objects: Vec<Object>,
}

impl Sector {
    pub fn parse(data: &[u8]) -> Result<Self, SectorError> {
        // The object count is read first, from the last 4 bytes, as the engine does.
        let count_at = data.len().checked_sub(4).ok_or(SectorError::Truncated { offset: 0, what: "object count" })?;
        let declared = Reader { data, pos: count_at }.u32("object count")?;

        let mut r = Reader { data, pos: 0 };
        let light_count = r.u32("light count")? as usize;
        let light_bytes = r.bytes(light_count.saturating_mul(LIGHT_SIZE), "lights")?;
        let lights = light_bytes.as_chunks::<LIGHT_SIZE>().0.iter().map(|c| read_light(c)).collect();

        let tile_bytes = r.bytes(TILE_COUNT * 4, "tiles")?;
        let tiles: Box<[Tile; TILE_COUNT]> = words(tile_bytes).map(Tile).collect::<Vec<_>>().into_boxed_slice().try_into().unwrap();

        let roofs = if r.u32("roof flag")? == 0 {
            let roof_bytes = r.bytes(ROOF_COUNT * 4, "roofs")?;
            Some(words(roof_bytes).map(ArtId).collect::<Vec<_>>().into_boxed_slice().try_into().unwrap())
        } else {
            None
        };

        let version_at = r.pos;
        let value = r.u32("version")?;
        let version = value.wrapping_sub(VERSION_BASE);
        if version > MAX_VERSION {
            return Err(SectorError::BadVersion { offset: version_at, value });
        }

        let mut tile_scripts = Vec::new();
        if version >= 1 {
            let n = r.u32("tile script count")? as usize;
            let bytes = r.bytes(n.saturating_mul(TILE_SCRIPT_SIZE), "tile scripts")?;
            tile_scripts = bytes
                .as_chunks::<TILE_SCRIPT_SIZE>()
                .0
                .iter()
                .map(|c| {
                    let w: Vec<u32> = words(c).collect();
                    TileScript { node_flags: w[0], tile: w[1], script: ScriptHeader { flags: w[2], counters: w[3], number: w[4] } }
                })
                .collect();
        }
        let mut script = ScriptHeader::default();
        if version >= 2 {
            script = ScriptHeader { flags: r.u32("sector script")?, counters: r.u32("sector script")?, number: r.u32("sector script")? };
        }
        let (mut townmap_info, mut aptitude_adjustment, mut light_scheme, mut sound_list) = (0, 0, 0, [0; 3]);
        if version >= 3 {
            townmap_info = r.u32("townmap info")?;
            aptitude_adjustment = r.u32("aptitude adjustment")? as i32;
            light_scheme = r.u32("light scheme")?;
            sound_list = [r.u32("sound list")?, r.u32("sound list")?, r.u32("sound list")?];
        }
        let mut blocked = None;
        if version >= 4 {
            let bytes = r.bytes(BLOCKED_WORDS * 4, "tile blocking")?;
            blocked = Some(words(bytes).collect::<Vec<_>>().into_boxed_slice().try_into().unwrap());
        }

        // An object takes at least 58 bytes; do not trust a corrupt count for the allocation.
        let mut objects = Vec::with_capacity((declared as usize).min(data.len() / 58));
        for index in 0..declared as usize {
            let offset = r.pos;
            let (object, used) =
                Object::parse_instance_prefix(&data[offset..]).map_err(|error| SectorError::Object { index, offset, error })?;
            objects.push(object);
            r.pos += used;
        }
        let after_objects = r.u32("object count")?;
        if after_objects != declared {
            return Err(SectorError::CountMismatch { declared, after_objects });
        }
        if r.pos != data.len() {
            return Err(SectorError::TrailingBytes { offset: r.pos, total: data.len() });
        }
        Ok(Self {
            lights,
            tiles,
            roofs,
            version,
            tile_scripts,
            script,
            townmap_info,
            aptitude_adjustment,
            light_scheme,
            sound_list,
            blocked,
            objects,
        })
    }

    /// The tile at `(x, y)`, both `< 64`.
    pub fn tile(&self, x: u32, y: u32) -> Tile {
        self.tiles[tile_index(x, y)]
    }

    /// Whether the tile blocking mask marks `(x, y)`, both `< 64`; `None` when the file has no
    /// mask (version < 4). This is the tile mask only: walls, scenery, closed portals and their
    /// prototype flags block too and are not decoded yet, so `Some(false)` does not mean walkable.
    pub fn tile_mask_blocked(&self, x: u32, y: u32) -> Option<bool> {
        let i = tile_index(x, y);
        self.blocked.as_ref().map(|m| m[i / 32] >> (i % 32) & 1 == 1)
    }
}

fn words(bytes: &[u8]) -> impl Iterator<Item = u32> + '_ {
    bytes.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c))
}

fn read_light(b: &[u8]) -> Light {
    let w: Vec<u32> = words(&b[8..0x20]).collect();
    Light {
        handle: u64::from_le_bytes(b[0..8].try_into().unwrap()),
        x: w[0] as i32,
        y: w[1] as i32,
        offset_x: w[2] as i32,
        offset_y: w[3] as i32,
        flags: w[4],
        art_id: ArtId(w[5]),
        color: [b[0x20], b[0x21], b[0x22]],
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], SectorError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or(SectorError::Truncated { offset: self.pos, what })?;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u32(&mut self, what: &'static str) -> Result<u32, SectorError> {
        Ok(u32::from_le_bytes(self.bytes(4, what)?.try_into().unwrap()))
    }
}

fn tile_index(x: u32, y: u32) -> usize {
    assert!(x < SECTOR_TILES && y < SECTOR_TILES, "tile ({x}, {y}) is outside the sector");
    (y * SECTOR_TILES + x) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{TYPE_PORTAL, TYPE_SCENERY, TYPE_WALL, Value};
    use crate::test_util::{ObjectBuilder, SectorBuilder};

    #[test]
    fn empty_sector_is_16440_bytes_and_parses() {
        let data = SectorBuilder { version: 3, ..SectorBuilder::new() }.build();
        assert_eq!(data.len(), 16440);
        let s = Sector::parse(&data).unwrap();
        assert_eq!(s.version, 3);
        assert!(s.lights.is_empty() && s.objects.is_empty() && s.tile_scripts.is_empty());
        assert_eq!(s.roofs, None);
    }

    #[test]
    fn each_version_has_its_blocks() {
        for (version, len) in [(0, 16400), (1, 16404), (2, 16416), (3, 16440), (4, 16952)] {
            let mut b = SectorBuilder { version, ..SectorBuilder::new() };
            b.script = ScriptHeader { flags: 1, counters: 2, number: 3 };
            b.tile_scripts = vec![TileScript { node_flags: 5, tile: 4095, script: ScriptHeader { flags: 6, counters: 7, number: 8 } }];
            b.townmap_info = 12;
            b.aptitude_adjustment = -20;
            b.light_scheme = 9;
            b.sound_list = [1, 2, 3];
            b.blocked[127] = 0x8000_0000;
            let data = b.build();
            let extra = if version >= 1 { 24 } else { 0 };
            assert_eq!(data.len(), len + extra, "version {version}");
            let s = Sector::parse(&data).unwrap_or_else(|e| panic!("version {version}: {e}"));
            assert_eq!(s.version, version);
            assert_eq!(s.tile_scripts.len(), usize::from(version >= 1));
            assert_eq!(s.script.number, if version >= 2 { 3 } else { 0 });
            assert_eq!(
                (s.townmap_info, s.aptitude_adjustment, s.light_scheme, s.sound_list),
                if version >= 3 { (12, -20, 9, [1, 2, 3]) } else { (0, 0, 0, [0; 3]) }
            );
            assert_eq!(s.tile_mask_blocked(63, 63), (version >= 4).then_some(true));
            assert_eq!(s.blocked.is_some(), version >= 4);
        }
    }

    #[test]
    fn bad_version_is_rejected() {
        for value in [VERSION_BASE + 5, 0x1234_5678, 4] {
            let data = SectorBuilder { version_word: Some(value), ..SectorBuilder::new() }.build();
            assert_eq!(Sector::parse(&data), Err(SectorError::BadVersion { offset: 4 + 16384 + 4, value }));
        }
    }

    #[test]
    fn lights_are_read() {
        let light = Light { handle: 0, x: 194, y: 82, offset_x: -3, offset_y: 7, flags: 0x18, art_id: ArtId(0x9028_0000), color: [255, 128, 1] };
        let data = SectorBuilder { lights: vec![light, Light::default()], ..SectorBuilder::new() }.build();
        let s = Sector::parse(&data).unwrap();
        assert_eq!(s.lights, vec![light, Light::default()]);
    }

    #[test]
    fn tiles_are_row_major() {
        let mut b = SectorBuilder::new();
        for (i, t) in b.tiles.iter_mut().enumerate() {
            *t = i as u32;
        }
        let s = Sector::parse(&b.build()).unwrap();
        assert_eq!(s.tile(0, 0), Tile(0));
        assert_eq!(s.tile(3, 2), Tile(2 * 64 + 3));
        assert_eq!(s.tile(63, 63), Tile(4095));
    }

    #[test]
    fn roofs_follow_the_empty_flag() {
        let roofs: Vec<ArtId> = (0..256).map(|i| ArtId(0x1000 + i)).collect();
        let s = Sector::parse(&SectorBuilder { roofs: Some(roofs.iter().map(|r| r.0).collect()), ..SectorBuilder::new() }.build()).unwrap();
        assert_eq!(s.roofs.as_deref().map(|r| r.to_vec()), Some(roofs));
    }

    #[test]
    fn blocked_bit_matches_tile() {
        let mut b = SectorBuilder::new();
        // Tile (5, 1) = index 69 = word 2, bit 5.
        b.blocked[2] = 1 << 5;
        let s = Sector::parse(&b.build()).unwrap();
        assert_eq!(s.tile_mask_blocked(5, 1), Some(true));
        let blocked: Vec<_> = (0..64).flat_map(|y| (0..64).map(move |x| (x, y))).filter(|&(x, y)| s.tile_mask_blocked(x, y) == Some(true)).collect();
        assert_eq!(blocked, vec![(5, 1)]);
    }

    #[test]
    fn tile_fields_decode() {
        // The commonest tile in the data (the void tile).
        let t = Tile(0x071c_01c0);
        assert_eq!((t.art_type(), t.terrain_1(), t.terrain_2(), t.edge(), t.variation(), t.flags()), (0, 28, 28, 0, 0, 7));
        assert!(!t.is_flipped() && !t.is_facade());
        let t = Tile((1 << 22) | (2 << 16) | (0xa << 12) | (5 << 9) | 1);
        assert_eq!((t.terrain_1(), t.terrain_2(), t.edge(), t.variation(), t.flags(), t.is_flipped()), (1, 2, 0xa, 5, 0, true));
        assert!(Tile(0xB000_0000).is_facade());
    }

    fn three_objects() -> Vec<Vec<u8>> {
        vec![
            ObjectBuilder::new(TYPE_WALL).field("current_aid", Value::Int32(10)).field("wall_flags", Value::Int32(1)).build(),
            ObjectBuilder::new(TYPE_SCENERY).field("location", Value::Int64(Some(99))).build(),
            ObjectBuilder::new(TYPE_PORTAL).field("portal_flags", Value::Int32(2)).field("scripts_idx", Value::Array(None)).build(),
        ]
    }

    #[test]
    fn object_extents_are_found() {
        let data = SectorBuilder { objects: three_objects(), ..SectorBuilder::new() }.build();
        let s = Sector::parse(&data).unwrap();
        let types: Vec<_> = s.objects.iter().map(|o| o.object_type).collect();
        assert_eq!(types, [TYPE_WALL, TYPE_SCENERY, TYPE_PORTAL]);
        assert_eq!(s.objects[0].art_id(), Some(ArtId(10)));
        assert_eq!(s.objects[1].field("location"), Some(&Value::Int64(Some(99))));
        assert_eq!(s.objects[2].int("portal_flags"), Some(2));
    }

    #[test]
    fn bad_object_reports_its_index_and_offset() {
        let mut objects = three_objects();
        let first_len = objects[0].len();
        objects[1][52] = 99;
        let b = SectorBuilder { objects, ..SectorBuilder::new() };
        let start = b.objects_offset();
        assert_eq!(
            Sector::parse(&b.build()),
            Err(SectorError::Object { index: 1, offset: start + first_len, error: ObjectError::UnsupportedType(99) })
        );
    }

    #[test]
    fn count_after_objects_must_match() {
        // The count at EOF-4 says 1, so the reader stops after one object and finds the second
        // object's version word where the repeated count should be.
        let data = SectorBuilder { objects: three_objects(), count: Some(1), ..SectorBuilder::new() }.build();
        assert_eq!(Sector::parse(&data), Err(SectorError::CountMismatch { declared: 1, after_objects: 0x77 }));
    }

    #[test]
    fn trailing_bytes_are_an_error() {
        // Another copy of the count after the real one: the counts agree, 4 bytes are left over.
        let mut data = SectorBuilder::new().build();
        let len = data.len();
        data.extend(0u32.to_le_bytes());
        assert_eq!(Sector::parse(&data), Err(SectorError::TrailingBytes { offset: len, total: len + 4 }));
    }

    #[test]
    fn truncated_block_names_the_block() {
        let b = SectorBuilder { lights: vec![Light::default()], ..SectorBuilder::new() };
        let data = b.build();
        // Cut inside the tiles.
        assert_eq!(Sector::parse(&data[..4 + 48 + 100]), Err(SectorError::Truncated { offset: 4 + 48, what: "tiles" }));
        assert_eq!(Sector::parse(&data[..2]), Err(SectorError::Truncated { offset: 0, what: "object count" }));
        // Every truncation fails cleanly.
        let data = SectorBuilder { objects: three_objects(), ..SectorBuilder::new() }.build();
        for len in 0..data.len() {
            assert!(Sector::parse(&data[..len]).is_err(), "{len}");
        }
    }
}
