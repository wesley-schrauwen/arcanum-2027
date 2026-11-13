//! Map list (`Rules/MapList.mes`), `map.prp` and sector coordinates.

use std::fmt;

use crate::mes::MesFile;

/// Path of the map list inside the module archive.
pub const MAP_LIST: &str = "Rules/MapList.mes";

/// Tiles along one side of a sector.
pub const SECTOR_TILES: u32 = 64;

const SECTOR_Y_SHIFT: u32 = 26;
const SECTOR_X_MASK: u64 = (1 << SECTOR_Y_SHIFT) - 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapError {
    Truncated { offset: usize, what: &'static str },
    BadEntry { id: u32, text: String },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { offset, what } => write!(f, "truncated at offset {offset}: {what}"),
            Self::BadEntry { id, text } => write!(f, "bad map list entry {id}: {text:?}"),
        }
    }
}

impl std::error::Error for MapError {}

/// A sector id: `(sector_y << 26) | sector_x`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SectorId(pub u64);

impl SectorId {
    pub fn new(x: u32, y: u32) -> Self {
        Self(((y as u64) << SECTOR_Y_SHIFT) | x as u64)
    }

    pub fn x(self) -> u32 {
        (self.0 & SECTOR_X_MASK) as u32
    }

    pub fn y(self) -> u32 {
        (self.0 >> SECTOR_Y_SHIFT) as u32
    }

    /// `<id>.sec`, the sector's file name inside the map folder.
    pub fn file_name(self) -> String {
        format!("{}.sec", self.0)
    }
}

/// A tile position on a map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileLoc {
    pub x: u32,
    pub y: u32,
}

impl TileLoc {
    pub fn sector(self) -> SectorId {
        SectorId::new(self.x / SECTOR_TILES, self.y / SECTOR_TILES)
    }

    pub fn tile_in_sector(self) -> (u32, u32) {
        (self.x % SECTOR_TILES, self.y % SECTOR_TILES)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKind {
    Normal,
    Start,
    Shopping,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapEntry {
    pub id: u32,
    /// Folder name under `maps/`, exactly as written (spaces and case kept).
    pub name: String,
    pub start: TileLoc,
    pub kind: MapKind,
    pub world_map: Option<i32>,
    pub area: Option<i32>,
}

/// Parse `Rules/MapList.mes`. Entries are `name, x, y[, Key: value]...`; unknown keys are ignored.
pub fn parse_map_list(mes: &MesFile) -> Result<Vec<MapEntry>, MapError> {
    let mut ids: Vec<u32> = mes.messages.keys().copied().collect();
    ids.sort_unstable();
    ids.into_iter().map(|id| parse_entry(id, &mes.messages[&id])).collect()
}

fn parse_entry(id: u32, text: &str) -> Result<MapEntry, MapError> {
    let bad = || MapError::BadEntry { id, text: text.to_string() };
    let mut parts = text.split(',').map(str::trim);
    let name = parts.next().filter(|n| !n.is_empty()).ok_or_else(bad)?.to_string();
    let x = parts.next().and_then(|s| s.parse().ok()).ok_or_else(bad)?;
    let y = parts.next().and_then(|s| s.parse().ok()).ok_or_else(bad)?;
    let mut entry = MapEntry {
        id,
        name,
        start: TileLoc { x, y },
        kind: MapKind::Normal,
        world_map: None,
        area: None,
    };
    for part in parts {
        let Some((key, value)) = part.split_once(':') else { continue };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "type" => match value.to_ascii_uppercase().as_str() {
                "START_MAP" => entry.kind = MapKind::Start,
                "SHOPPING_MAP" => entry.kind = MapKind::Shopping,
                _ => {}
            },
            "worldmap" => entry.world_map = Some(value.parse().map_err(|_| bad())?),
            "area" => entry.area = Some(value.parse().map_err(|_| bad())?),
            _ => {}
        }
    }
    Ok(entry)
}

/// The entry with `Type: START_MAP`, if any.
pub fn start_map(list: &[MapEntry]) -> Option<&MapEntry> {
    list.iter().find(|e| e.kind == MapKind::Start)
}

/// Contents of `map.prp`: 24 bytes, little-endian.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapProperties {
    pub base_terrain: u32,
    /// Meaning unknown (1 in `Arcanum1-024-fixed`).
    pub unknown: u32,
    /// Width in tiles.
    pub width: u64,
    /// Height in tiles.
    pub height: u64,
}

impl MapProperties {
    pub fn parse(data: &[u8]) -> Result<Self, MapError> {
        if data.len() < 24 {
            return Err(MapError::Truncated { offset: data.len(), what: "map.prp" });
        }
        let u32_at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let u64_at = |o: usize| u64::from_le_bytes(data[o..o + 8].try_into().unwrap());
        Ok(Self { base_terrain: u32_at(0), unknown: u32_at(4), width: u64_at(8), height: u64_at(16) })
    }

    pub fn contains(&self, loc: TileLoc) -> bool {
        (loc.x as u64) < self.width && (loc.y as u64) < self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let digits: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        digits
            .chunks(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn sector_id_packs_y_high() {
        let id = SectorId::new(1452, 1290);
        assert_eq!(id.0, 86_570_436_012);
        assert_eq!((id.x(), id.y()), (1452, 1290));
        assert_eq!(SectorId(67_108_865).file_name(), "67108865.sec");
    }

    #[test]
    fn tile_to_sector() {
        let t = TileLoc { x: 92958, y: 82592 };
        assert_eq!(t.sector(), SectorId::new(1452, 1290));
        assert_eq!(t.tile_in_sector(), (30, 32));
    }

    fn list() -> Vec<MapEntry> {
        let mes = MesFile::parse(
            b"// maps\r\n{5000}{Arcanum1-024-fixed, 92958,82592, Type: START_MAP, WorldMap: 0}\r\n\
              {5007}{Void, 1055, 1055}\r\n\
              {5011}{Bates Mansion Lev 1, 104, 92, WorldMap: 0, Area: 21}\r\n\
              {5013}{ShopMap, 31, 29, Type: SHOPPING_MAP}\r\n\
              {5014}{Spaced, 96, 126, Colour: red}\r\n",
        )
        .unwrap();
        parse_map_list(&mes).unwrap()
    }

    #[test]
    fn map_list_entries() {
        let list = list();
        let ids: Vec<u32> = list.iter().map(|e| e.id).collect();
        assert_eq!(ids, [5000, 5007, 5011, 5013, 5014]);
        let e = &list[0];
        assert_eq!(e.name, "Arcanum1-024-fixed");
        assert_eq!(e.start, TileLoc { x: 92958, y: 82592 });
        assert_eq!((e.kind, e.world_map, e.area), (MapKind::Start, Some(0), None));
        let e = &list[2];
        assert_eq!(e.name, "Bates Mansion Lev 1");
        assert_eq!(e.start, TileLoc { x: 104, y: 92 });
        assert_eq!((e.kind, e.world_map, e.area), (MapKind::Normal, Some(0), Some(21)));
        assert_eq!(list[3].kind, MapKind::Shopping);
        assert_eq!(list[1].kind, MapKind::Normal);
        assert_eq!(list[4].start, TileLoc { x: 96, y: 126 }); // unknown key ignored
        assert_eq!(start_map(&list).unwrap().id, 5000);
        assert!(start_map(&list[1..]).is_none());
    }

    #[test]
    fn bad_map_entry_is_an_error() {
        for text in [&b"{5099}{Broken, abc, 3}"[..], b"{5099}{Broken, 1}", b"{5099}{, 1, 2}", b"{5099}{M, 1, 2, Area: x}"] {
            let mes = MesFile::parse(text).unwrap();
            assert!(matches!(parse_map_list(&mes), Err(MapError::BadEntry { id: 5099, .. })), "{text:?}");
        }
    }

    #[test]
    fn map_properties() {
        let p = MapProperties::parse(&hex("02000000 01000000 00f40100 00000000 00f40100 00000000")).unwrap();
        assert_eq!((p.base_terrain, p.unknown, p.width, p.height), (2, 1, 128_000, 128_000));
        assert!(p.contains(TileLoc { x: 127_999, y: 0 }));
        assert!(!p.contains(TileLoc { x: 128_000, y: 0 }));
        assert!(!p.contains(TileLoc { x: 0, y: 128_000 }));
        assert!(matches!(MapProperties::parse(&[0; 23]), Err(MapError::Truncated { .. })));
    }

    #[test]
    fn errors_display() {
        let e = MapError::BadEntry { id: 1, text: "x".into() };
        assert_eq!(e.to_string(), "bad map list entry 1: \"x\"");
    }
}
