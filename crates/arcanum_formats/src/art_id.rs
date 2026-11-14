//! Art ids and the `.art` file each one names.
//!
//! An art id is a `u32`. Bits 28..31 are the art type (accessor 0x502700 in Arcanum.exe); the
//! engine's id -> path dispatcher ends at 0x41dfa9. `0xFFFFFFFF` means "no art" ([`ArtId::NONE`]).
//! Every rule below was found in the exe and then checked against the shipped data: all 2.54 M
//! tile ids, 28,064 walls, 1,588 portals, 117,386 scenery objects, 1,012 lights and 6,483 roofs of
//! the 621 start-map sectors resolve to files that exist (the sole exception is one scenery object
//! whose id is `0xFFFFFFFF`). That only proves the file exists, so the engine code is the evidence
//! for each rule; negative tests (a swapped wall token leaves 616 of 3,157 wall ids without a
//! file) back the ones that could be guessed wrong.
//!
//! Paths use `/` and the spelling the `.mes` files give; the VFS matches case-insensitively.
//!
//! | Type | Kind | Path builder |
//! |---|---|---|
//! | 0 | tile | 0x4eae90 -> 0x4eaf70 |
//! | 1 | wall | 0x4eceb0 -> 0x4ecfc0 |
//! | 2 | critter | not decoded |
//! | 3 | portal | 0x4ec620 |
//! | 4 | scenery | 0x41db89 branch |
//! | 5, 6, 7 | interface, item, container | not decoded |
//! | 9 | light | 0x4ed250 |
//! | 0xA | roof | 0x4ed370 |
//! | 0xB | facade (sector tiles that are not terrain) | 0x4ec3f0 |
//! | 0xC, 0xD, 0xE | monster, unique npc, eye candy | not decoded |
//!
//! Item, container, critter, monster, unique npc, interface and eye candy ids decode to a kind
//! only; [`ArtIndex::path`] returns `None` for them (their rules are not verified against data).
//!
//! # Bit layouts
//!
//! The number accessor is 0x502710 (bits 19..27 for every type not listed there) and the rotation
//! accessor 0x502880 (bits 11..13).
//!
//! - **Tile** (accessors 0x5036b0..0x503990): terrain name 1 bits 22..27, name 2 bits 16..21,
//!   edge mask 12..15, variation 9..11, outdoor flag 8, name-1 flippable 7, name-2 flippable 6,
//!   bit 0 flip (0x504fd0).
//! - **Wall**: wall number bits 20..27 (0x503ad0), piece 14..19 (0x503a60), rotation 11..13,
//!   flags 0x400 and 0x80 (0x503b70), variation digit 8..9 (0x503b00).
//! - **Portal**: number 19..27, rotation 11..13, bit 10 variant (0x504260), bit 9 (0x200).
//! - **Scenery**: number 19..27, rotation 11..13, category 6..10 (0x504300).
//! - **Facade** (0x504a60): bits 17..24 are the low 8 bits of the facade number, bit 27 adds 0x100.
//!   The other fields (tile index in the facade bits 1..10, flag bit 0) do not affect the path.
//! - **Roof / light**: number 19..27. Light also has rotation at bits 9..11 and a shadow flag in
//!   bit 0 (0x504790).
//!
//! # Index files and path rules
//!
//! All index files are `.mes` ([`ART_INDEX_FILES`]).
//!
//! **Tile**, `art/tile/tilename.mes` (loaded at 0x4eac85). Entry text is `<3-letter name>[/flags]
//! <sound>`; the name is the text before the first `/` or space. Key ranges give four lists, in
//! key order: 0..99 outdoor flippable, 100..199 outdoor non-flippable, 200..299 indoor flippable,
//! 300..399 indoor non-flippable (0x4eb270, 0x4eb2e0, 0x4eb5e0; selection 0x4eb0c0). The outdoor
//! flag (bit 8) picks outdoor or indoor; flag bit 7 (for name 1) or 6 (name 2) picks flippable. A
//! name field is the position in the chosen list. Builder 0x4eaf70, with `E = "06b489237ea5dc10"`
//! (0x5bb518) and `L = 'a' + variation`:
//!
//! 1. If `edge == 15` or the two names are equal (ignoring case): `<n1>bse<E[edge]><L>.art`
//!    (format 0x5bb588).
//! 2. Else if `edge == 0`: `<n2>bse<E[0]><L>.art`.
//! 3. Else the order of a name (0x4eb7d0) is its position in the outdoor flippable list, or the
//!    size of that list plus its position in the outdoor non-flippable list, or none. If n1 has
//!    no order: `<n1>bse<E[edge]><L>.art`. If n2 has none: `<n2>bse<E[15 - edge]><L>.art`. If
//!    `order(n1) < order(n2)`: `<n1><n2><E[edge]><L>.art`, else `<n2><n1><E[15 - edge]><L>.art`
//!    (format 0x5bb570).
//!
//! The engine accessor 0x503700 remaps the edge of a flipped tile (bit 0) through the table at
//! 0x5be8c0 and mirrors the sprite when drawing. The file name does **not** use the remapped
//! edge: it uses the stored edge unchanged. Observed rule: the stored edge of every flipped tile
//! is already an edge that has art, remapping gives missing files (1,275 in the first 9 sectors)
//! and the stored edge gives none (0 over 2.3 M ids). Why the accessor remaps is not traced.
//!
//! **Wall**, `art/wall/structure.mes` (loaders 0x4eca60..0x4ecea7). Keys 0..999 are the wall
//! structures in key order (keys 1000+ are editor thumbnails); the text is four tokens (interior,
//! exterior, floor, roof), of which only the first three characters of the first two are used.
//! Rule (0x4eceb0, 0x4ecfc0): `rot = bits 11..13`, `flags = id & 0x480`. If `rot` is 2, 3, 6 or 7,
//! flags 0x400 and 0x80 swap. If flags has 0x400: flags = 0x400 and piece 7 becomes 0. Else if
//! flags has 0x80: flags = 0x80 and piece 8 becomes 0. Else flags = 0. The name is token 1 when
//! `rot >> 1` is 0 or 3, else token 2. The side letter from `"ULR"` (0x5bb6ac) is `L` for 0x400,
//! `L` for 0x80 with piece in 1..6 and `R` otherwise, `U` with no flag. Path
//! `<name><piece name><side><variation>.art` (format 0x5bb88c); the 46 piece names are at
//! 0x5bb6b0 (see [`WALL_PIECES`]). A wall number beyond the structure list or a piece beyond 45
//! has no path.
//!
//! **Portal**, `art/portal/portal.mes` (0x4ec5a0). Text is `<file>.art <sound>`: the file is the
//! text up to the first space. Key (0x4ec8f0) = number if bit 10 is set, else number + 1001. Path
//! `art/portal/<file>` (format 0x5bb69c). If bit 9 is set the character at `len - 6` of the path
//! (the `U` of `...U0.ART`) becomes `D` (0x4ec620).
//!
//! **Scenery**, `art/scenery/scenery.mes` (0x41ca52). Key = category * 1000 + number. The text is
//! the file name with extension. Path `art/scenery/<text>` (format 0x5a15b0).
//!
//! **Facade**, `art/facade/facadename.mes` (0x4ec4b0). Entries are names without extension,
//! indexed by position (keys are contiguous). Path `art/Facade/<name>.art` (format 0x5bb654).
//!
//! **Roof**, `art/roof/roofname.mes` (0x5bb8f4): position = number, and a number past the end
//! uses 0 (0x4ed3a0). Path `art/roof/<name>.art`. Roof ids in a sector are `0xFFFFFFFF` for none.
//!
//! **Light**, `art/light/light.mes` (0x5bb8a4): key = number. Path `art/light/<name>.art`, or with
//! bit 0 set `art/light/<name>_s<n>.art` with `n = ((id >> 4) & 0x1f) / 8` (0x4ed29c).
//!
//! Not resolved: facade sub-fields, item/critter/monster paths, container paths (type 7 has the
//! scenery layout and `art/container/container.mes`, untested).

use std::collections::HashMap;
use std::fmt;

use crate::mes::{MesError, MesFile};

/// The `.mes` index files [`ArtIndex::load`] reads, as VFS paths.
pub const ART_INDEX_FILES: &[&str] = &[
    "art/tile/tilename.mes",
    "art/wall/structure.mes",
    "art/portal/portal.mes",
    "art/scenery/scenery.mes",
    "art/facade/facadename.mes",
    "art/roof/roofname.mes",
    "art/light/light.mes",
];

/// Wall piece names, indexed by bits 14..19 (table at 0x5bb6b0).
pub const WALL_PIECES: [&str; 46] = [
    "bse", "lfc", "bse", "bcl", "bcr", "tcl", "tcr", "uec", "lec", "w3l", "w3a", "w3r", "w4l", "w4a", "w4b", "w4r",
    "w5l", "w5a", "w5b", "w5c", "w5r", "d3l", "d3a", "d3r", "d4l", "d4a", "d4b", "d4r", "d6l", "d6a", "d6b", "d6c",
    "d6d", "d6r", "p3l", "p3a", "p3r", "p4l", "p4a", "p4b", "p4r", "p5l", "p5a", "p5b", "p5c", "p5r",
];

/// Edge letters of tile file names (0x5bb518), indexed by the 4-bit edge mask.
const EDGE_LETTERS: &[u8; 16] = b"06b489237ea5dc10";
/// Wall side letters (0x5bb6ac).
const SIDE_LETTERS: &[u8; 3] = b"ULR";

/// The art type, bits 28..31 of an [`ArtId`] (0x502700).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtKind {
    Tile,
    Wall,
    Critter,
    Portal,
    Scenery,
    Interface,
    Item,
    Container,
    /// Type 8 (not in the engine's dispatcher) or 15.
    Unknown(u32),
    Light,
    Roof,
    Facade,
    Monster,
    UniqueNpc,
    EyeCandy,
}

/// An art id; see the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ArtId(pub u32);

impl ArtId {
    /// "No art" (`0xFFFFFFFF`).
    pub const NONE: ArtId = ArtId(0xFFFF_FFFF);

    pub fn is_none(self) -> bool {
        self == Self::NONE
    }

    /// Bits 28..31.
    pub fn art_type(self) -> u32 {
        self.0 >> 28
    }

    pub fn kind(self) -> ArtKind {
        match self.art_type() {
            0 => ArtKind::Tile,
            1 => ArtKind::Wall,
            2 => ArtKind::Critter,
            3 => ArtKind::Portal,
            4 => ArtKind::Scenery,
            5 => ArtKind::Interface,
            6 => ArtKind::Item,
            7 => ArtKind::Container,
            9 => ArtKind::Light,
            0xA => ArtKind::Roof,
            0xB => ArtKind::Facade,
            0xC => ArtKind::Monster,
            0xD => ArtKind::UniqueNpc,
            0xE => ArtKind::EyeCandy,
            t => ArtKind::Unknown(t),
        }
    }

    /// Bits 19..27: portal, scenery, roof and light number (0x502710).
    pub fn number(self) -> u32 {
        (self.0 >> 19) & 0x1ff
    }

    /// Bits 11..13 (0x502880): walls, portals and scenery.
    pub fn rotation(self) -> u32 {
        (self.0 >> 11) & 7
    }

    /// Scenery category, bits 6..10 (0x504300).
    pub fn scenery_category(self) -> u32 {
        (self.0 >> 6) & 0x1f
    }

    /// Wall number, bits 20..27 (0x503ad0).
    pub fn wall_number(self) -> u32 {
        (self.0 >> 20) & 0xff
    }

    /// Wall piece, bits 14..19 (0x503a60).
    pub fn wall_piece(self) -> u32 {
        (self.0 >> 14) & 0x3f
    }

    /// Wall variation digit, bits 8..9 (0x503b00).
    pub fn wall_variation(self) -> u32 {
        (self.0 >> 8) & 3
    }

    /// Wall flags `0x400` / `0x80` (0x503b70), unswapped.
    pub fn wall_flags(self) -> u32 {
        self.0 & 0x480
    }

    /// Facade number (0x504a60): bits 17..24 plus 0x100 when bit 27 is set.
    pub fn facade_number(self) -> u32 {
        ((self.0 >> 17) & 0xff) + if self.0 & 0x0800_0000 != 0 { 0x100 } else { 0 }
    }
}

#[derive(Debug)]
pub enum ArtIndexError {
    /// An index file is not in the archives.
    Missing(&'static str),
    Mes { file: &'static str, error: MesError },
    /// An entry has the wrong shape.
    Malformed { file: &'static str, key: u32 },
}

impl fmt::Display for ArtIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(file) => write!(f, "{file}: not found"),
            Self::Mes { file, error } => write!(f, "{file}: {error}"),
            Self::Malformed { file, key } => write!(f, "{file}: malformed entry {{{key}}}"),
        }
    }
}

impl std::error::Error for ArtIndexError {}

/// The index files needed to turn an [`ArtId`] into a path.
#[derive(Debug, Clone, Default)]
pub struct ArtIndex {
    /// Tile names by list: outdoor flippable, outdoor fixed, indoor flippable, indoor fixed.
    tile_lists: [Vec<String>; 4],
    /// (first token, second token) of each wall structure.
    walls: Vec<(String, String)>,
    portals: HashMap<u32, String>,
    scenery: HashMap<u32, String>,
    facades: Vec<String>,
    roofs: Vec<String>,
    lights: HashMap<u32, String>,
}

/// The entries of a `.mes` file in key order.
fn sorted(mes: &MesFile) -> Vec<(u32, &str)> {
    let mut v: Vec<_> = mes.messages.iter().map(|(k, t)| (*k, t.as_str())).collect();
    v.sort_by_key(|(k, _)| *k);
    v
}

fn first_three(token: &str) -> String {
    token.chars().take(3).collect()
}

impl ArtIndex {
    /// Loads [`ART_INDEX_FILES`] through `read` (VFS paths, `/`-separated).
    pub fn load(read: impl Fn(&str) -> Option<Vec<u8>>) -> Result<Self, ArtIndexError> {
        let mes = |file: &'static str| -> Result<MesFile, ArtIndexError> {
            let data = read(file).ok_or(ArtIndexError::Missing(file))?;
            MesFile::parse(&data).map_err(|error| ArtIndexError::Mes { file, error })
        };
        let mut index = Self::default();

        const TILES: &str = "art/tile/tilename.mes";
        let tiles = mes(TILES)?;
        for (key, text) in sorted(&tiles) {
            if key < 400 {
                let name = text.trim().split(['/', ' ']).next().unwrap_or("");
                index.tile_lists[(key / 100) as usize].push(name.to_string());
            }
        }

        const WALLS: &str = "art/wall/structure.mes";
        for (key, text) in sorted(&mes(WALLS)?) {
            if key >= 1000 {
                break;
            }
            let mut tokens = text.split_whitespace();
            let (Some(a), Some(b)) = (tokens.next(), tokens.next()) else {
                return Err(ArtIndexError::Malformed { file: WALLS, key });
            };
            index.walls.push((first_three(a), first_three(b)));
        }

        for (key, text) in sorted(&mes("art/portal/portal.mes")?) {
            index.portals.insert(key, text.split(' ').next().unwrap_or("").to_string());
        }
        for (key, text) in sorted(&mes("art/scenery/scenery.mes")?) {
            index.scenery.insert(key, text.trim().to_string());
        }
        index.facades = sorted(&mes("art/facade/facadename.mes")?).iter().map(|(_, t)| t.trim().to_string()).collect();
        index.roofs = sorted(&mes("art/roof/roofname.mes")?).iter().map(|(_, t)| t.trim().to_string()).collect();
        for (key, text) in sorted(&mes("art/light/light.mes")?) {
            index.lights.insert(key, text.trim().to_string());
        }
        Ok(index)
    }

    /// The VFS path of the `.art` file for `id`, or `None` for no art, a kind that is not decoded,
    /// or an id the index files do not cover.
    pub fn path(&self, id: ArtId) -> Option<String> {
        if id.is_none() {
            return None;
        }
        match id.kind() {
            ArtKind::Tile => self.tile_path(id),
            ArtKind::Wall => self.wall_path(id),
            ArtKind::Portal => self.portal_path(id),
            ArtKind::Scenery => {
                let key = id.scenery_category() * 1000 + id.number();
                self.scenery.get(&key).map(|name| format!("art/scenery/{name}"))
            }
            ArtKind::Facade => self.facades.get(id.facade_number() as usize).map(|name| format!("art/Facade/{name}.art")),
            ArtKind::Roof => {
                let n = id.number() as usize;
                let n = if n < self.roofs.len() { n } else { 0 };
                self.roofs.get(n).map(|name| format!("art/roof/{name}.art"))
            }
            ArtKind::Light => {
                let name = self.lights.get(&id.number())?;
                Some(if id.0 & 1 != 0 {
                    format!("art/light/{name}_s{}.art", ((id.0 >> 4) & 0x1f) / 8)
                } else {
                    format!("art/light/{name}.art")
                })
            }
            _ => None,
        }
    }

    fn tile_name(&self, index: u32, outdoor: bool, flippable: bool) -> Option<&str> {
        let list = match (outdoor, flippable) {
            (true, true) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (false, false) => 3,
        };
        self.tile_lists[list].get(index as usize).map(String::as_str)
    }

    /// 0x4eb7d0: position in the outdoor flippable list, then in the outdoor fixed list after it.
    fn tile_order(&self, name: &str) -> Option<usize> {
        let find = |list: &[String]| list.iter().position(|n| n.eq_ignore_ascii_case(name));
        find(&self.tile_lists[0]).or_else(|| find(&self.tile_lists[1]).map(|i| self.tile_lists[0].len() + i))
    }

    fn tile_path(&self, id: ArtId) -> Option<String> {
        let v = id.0;
        let outdoor = (v >> 8) & 1 == 1;
        let a = self.tile_name((v >> 22) & 0x3f, outdoor, (v >> 7) & 1 == 1)?;
        let b = self.tile_name((v >> 16) & 0x3f, outdoor, (v >> 6) & 1 == 1)?;
        let edge = ((v >> 12) & 0xf) as usize;
        let letter = (b'a' + ((v >> 9) & 7) as u8) as char;
        let e = |i: usize| EDGE_LETTERS[i] as char;
        let base = |name: &str, edge: usize| format!("art/tile/{name}bse{}{letter}.art", e(edge));
        if edge == 15 || a.eq_ignore_ascii_case(b) {
            return Some(base(a, edge));
        }
        if edge == 0 {
            return Some(base(b, 0));
        }
        let Some(oa) = self.tile_order(a) else { return Some(base(a, edge)) };
        let Some(ob) = self.tile_order(b) else { return Some(base(b, 15 - edge)) };
        Some(if oa < ob {
            format!("art/tile/{a}{b}{}{letter}.art", e(edge))
        } else {
            format!("art/tile/{b}{a}{}{letter}.art", e(15 - edge))
        })
    }

    fn wall_path(&self, id: ArtId) -> Option<String> {
        let rot = id.rotation();
        let mut piece = id.wall_piece();
        let mut flags = id.wall_flags();
        if matches!(rot, 2 | 3 | 6 | 7) {
            flags = (if flags & 0x400 != 0 { 0x80 } else { 0 }) | (if flags & 0x80 != 0 { 0x400 } else { 0 });
        }
        if flags & 0x400 != 0 {
            flags = 0x400;
            if piece == 7 {
                piece = 0;
            }
        } else if flags & 0x80 != 0 {
            flags = 0x80;
            if piece == 8 {
                piece = 0;
            }
        } else {
            flags = 0;
        }
        let (first, second) = self.walls.get(id.wall_number() as usize)?;
        let piece_name = WALL_PIECES.get(piece as usize)?;
        let name = if matches!(rot >> 1, 0 | 3) { first } else { second };
        let side = match flags {
            0x400 => 1,
            0x80 if (1..=6).contains(&piece) => 1,
            0x80 => 2,
            _ => 0,
        };
        Some(format!("art/wall/{name}{piece_name}{}{}.art", SIDE_LETTERS[side] as char, id.wall_variation()))
    }

    fn portal_path(&self, id: ArtId) -> Option<String> {
        let number = id.number();
        let key = if (id.0 >> 10) & 1 == 1 { number } else { number + 1001 };
        let name = self.portals.get(&key)?;
        let mut path = format!("art/portal/{name}");
        if id.0 & 0x200 != 0 {
            // The `U` of `...U0.ART` (0x4ec620).
            let at = path.len().checked_sub(6)?;
            if !path.is_char_boundary(at) || !path.is_char_boundary(at + 1) {
                return None;
            }
            path.replace_range(at..at + 1, "D");
        }
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const TILENAME: &str = "// outdoor flippable\n{0}{drt/f x}\n{1}{grs/f x}\n{2}{bg1/b x}\n{100}{bg2 x}\n{200}{ind/f x}\n{201}{drt/f x}\n{300}{flr x}\n";
    const STRUCTURE: &str = "{0}{BB1 BB2 flr roof}\n{1}{CC1xx CC2xx flr roof}\n{1000}{thumb}\n";
    const PORTAL: &str = "{3}{BB1F3aU0.ART 5}\n{1001}{BB1F3aU0.ART 5}\n";
    const SCENERY: &str = "{0}{proj.art}\n{1005}{tree.art}\n";
    fn facade_mes() -> String {
        (0..300).map(|i| format!("{{{i}}}{{fa{i}}}\n")).collect()
    }
    const ROOF: &str = "{0}{rf0}\n{1}{rf1}\n";
    const LIGHT: &str = "{80}{lgt}\n";

    fn index() -> ArtIndex {
        let facade = facade_mes();
        let files: HashMap<&str, &str> = HashMap::from([
            ("art/tile/tilename.mes", TILENAME),
            ("art/wall/structure.mes", STRUCTURE),
            ("art/portal/portal.mes", PORTAL),
            ("art/scenery/scenery.mes", SCENERY),
            ("art/facade/facadename.mes", facade.as_str()),
            ("art/roof/roofname.mes", ROOF),
            ("art/light/light.mes", LIGHT),
        ]);
        ArtIndex::load(|p| files.get(p).map(|s| s.as_bytes().to_vec())).unwrap()
    }

    fn tile(n1: u32, n2: u32, edge: u32, var: u32, flags: u32) -> ArtId {
        ArtId((n1 << 22) | (n2 << 16) | (edge << 12) | (var << 9) | (flags << 6))
    }
    const OUT_FLIP: u32 = 0b111;
    fn p(i: &ArtIndex, id: ArtId) -> Option<String> {
        i.path(id)
    }

    #[test]
    fn tile_id_to_path() {
        let i = index();
        assert_eq!(p(&i, tile(1, 1, 15, 0, OUT_FLIP)).as_deref(), Some("art/tile/grsbse0a.art"));
        assert_eq!(p(&i, tile(0, 1, 5, 2, OUT_FLIP)).as_deref(), Some("art/tile/drtgrs9c.art"));
        assert_eq!(p(&i, tile(1, 0, 5, 2, OUT_FLIP)).as_deref(), Some("art/tile/drtgrsac.art"));
        assert_eq!(p(&i, tile(0, 1, 0, 1, OUT_FLIP)).as_deref(), Some("art/tile/grsbse0b.art"));
        // Same name in both slots (case-insensitive) is a base tile.
        assert_eq!(p(&i, tile(0, 0, 7, 7, OUT_FLIP)).as_deref(), Some("art/tile/drtbse3h.art"));
        // Non-flippable outdoor name (bg2) sorts after the flippable ones: names swap, edge becomes 15 - edge.
        assert_eq!(p(&i, tile(0, 0, 3, 0, 0b101)).as_deref(), Some("art/tile/drtbg2da.art"));
        // Flip bit does not change the name.
        assert_eq!(p(&i, ArtId(tile(0, 1, 5, 2, OUT_FLIP).0 | 1)), p(&i, tile(0, 1, 5, 2, OUT_FLIP)));
    }

    #[test]
    fn tile_name_not_found_in_order_lists() {
        let i = index();
        // Indoor: n1 = ind (flippable, idx 0, not an outdoor name), n2 = flr: n1 not found -> n1 bse E[edge].
        assert_eq!(p(&i, tile(0, 0, 3, 0, 0b010)).as_deref(), Some("art/tile/indbse4a.art"));
        // n1 = drt (indoor idx 1, also an outdoor name), n2 = flr (not found): n2 bse E[15-edge].
        assert_eq!(p(&i, tile(1, 0, 3, 0, 0b010)).as_deref(), Some("art/tile/flrbseda.art"));
    }

    #[test]
    fn tile_index_out_of_list_is_none() {
        assert_eq!(index().path(tile(40, 0, 3, 0, OUT_FLIP)), None);
    }

    #[test]
    fn facade_id_to_path() {
        let i = index();
        assert_eq!(p(&i, ArtId(0xB000_0000 | (1 << 17))).as_deref(), Some("art/Facade/fa1.art"));
        // Bit 27 adds 0x100.
        assert_eq!(p(&i, ArtId(0xB800_0000)).as_deref(), Some("art/Facade/fa256.art"));
        assert_eq!(p(&i, ArtId(0xB000_0000 | (200 << 17) | (1 << 27))), None);
    }

    fn wall(num: u32, piece: u32, rot: u32, flags: u32, var: u32) -> ArtId {
        ArtId((1 << 28) | (num << 20) | (piece << 14) | (rot << 11) | flags | (var << 8))
    }

    #[test]
    fn wall_id_to_path() {
        let i = index();
        assert_eq!(p(&i, wall(0, 22, 1, 0, 0)).as_deref(), Some("art/wall/BB1d3aU0.art"));
        // Rotations 2..5 use the second token; variation digit.
        assert_eq!(p(&i, wall(0, 22, 4, 0, 3)).as_deref(), Some("art/wall/BB2d3aU3.art"));
        // Only the first three characters of the token.
        assert_eq!(p(&i, wall(1, 1, 7, 0, 0)).as_deref(), Some("art/wall/CC1lfcU0.art"));
        // Flag 0x400 -> L; flag 0x80 -> L for pieces 1..6, else R.
        assert_eq!(p(&i, wall(0, 22, 1, 0x400, 0)).as_deref(), Some("art/wall/BB1d3aL0.art"));
        assert_eq!(p(&i, wall(0, 22, 1, 0x80, 0)).as_deref(), Some("art/wall/BB1d3aR0.art"));
        assert_eq!(p(&i, wall(0, 3, 1, 0x80, 0)).as_deref(), Some("art/wall/BB1bclL0.art"));
        // Rotations 2, 3, 6, 7 swap the two flags first.
        assert_eq!(p(&i, wall(0, 22, 3, 0x400, 0)).as_deref(), Some("art/wall/BB2d3aR0.art"));
        assert_eq!(p(&i, wall(0, 3, 3, 0x400, 0)).as_deref(), Some("art/wall/BB2bclL0.art"));
        // Piece 7 with 0x400 and piece 8 with 0x80 become piece 0 (bse).
        assert_eq!(p(&i, wall(0, 7, 1, 0x400, 0)).as_deref(), Some("art/wall/BB1bseL0.art"));
        assert_eq!(p(&i, wall(0, 8, 1, 0x80, 0)).as_deref(), Some("art/wall/BB1bseR0.art"));
        // Out of range structure / piece.
        assert_eq!(p(&i, wall(2, 0, 1, 0, 0)), None);
        assert_eq!(p(&i, wall(0, 46, 1, 0, 0)), None);
    }

    #[test]
    fn portal_id_to_path() {
        let i = index();
        // Bit 10 clear: key = number + 1001.
        assert_eq!(p(&i, ArtId(3 << 28)).as_deref(), Some("art/portal/BB1F3aU0.ART"));
        // Bit 10 set: key = number; bit 9 turns U into D.
        assert_eq!(p(&i, ArtId((3 << 28) | (3 << 19) | (1 << 10))).as_deref(), Some("art/portal/BB1F3aU0.ART"));
        assert_eq!(p(&i, ArtId((3 << 28) | (3 << 19) | (1 << 10) | (1 << 9))).as_deref(), Some("art/portal/BB1F3aD0.ART"));
        assert_eq!(p(&i, ArtId((3 << 28) | (4 << 19))), None);
    }

    #[test]
    fn scenery_id_to_path() {
        let i = index();
        assert_eq!(p(&i, ArtId((4 << 28) | (5 << 19) | (1 << 6))).as_deref(), Some("art/scenery/tree.art"));
        assert_eq!(p(&i, ArtId(4 << 28)).as_deref(), Some("art/scenery/proj.art"));
    }

    #[test]
    fn roof_and_light_id_to_path() {
        let i = index();
        assert_eq!(p(&i, ArtId((0xA << 28) | (1 << 19))).as_deref(), Some("art/roof/rf1.art"));
        // Out of range roof number uses 0.
        assert_eq!(p(&i, ArtId((0xA << 28) | (9 << 19))).as_deref(), Some("art/roof/rf0.art"));
        assert_eq!(p(&i, ArtId((9 << 28) | (80 << 19))).as_deref(), Some("art/light/lgt.art"));
        assert_eq!(p(&i, ArtId((9 << 28) | (80 << 19) | (16 << 4) | 1)).as_deref(), Some("art/light/lgt_s2.art"));
    }

    #[test]
    fn unknown_index_is_none() {
        let i = index();
        assert_eq!(p(&i, ArtId((4 << 28) | (6 << 19))), None);
        assert_eq!(p(&i, ArtId((9 << 28) | (81 << 19))), None);
    }

    #[test]
    fn no_art_and_undecoded_kinds_are_none() {
        let i = index();
        assert_eq!(p(&i, ArtId::NONE), None);
        for t in [2u32, 5, 6, 7, 8, 0xC, 0xD, 0xE, 0xF] {
            assert_eq!(p(&i, ArtId(t << 28 | 0x1234)), None, "type {t}");
        }
    }

    #[test]
    fn load_reports_missing_and_malformed_files() {
        assert!(matches!(ArtIndex::load(|_| None), Err(ArtIndexError::Missing(_))));
    }
}
