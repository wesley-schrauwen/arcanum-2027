# Map Formats Implementation Plan (World milestone, step 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read everything needed to draw and block the starting map: the map list and start
position, map properties, sector files (tiles, blocking, the sector object list), the object
types those sectors contain, and art id → `.art` path for tiles, walls, portals and scenery.

**Architecture:** New modules in `arcanum_formats` (no Bevy, no VFS dependency; parsers take
`&[u8]` or `&str`). Unknown layouts are recovered the way `object.rs` was: inspect the data,
confirm in Arcanum.exe with REA, then require every real file to parse **to the exact byte**.
Synthetic fixtures cover each rule; real-data tests (skipping without `ARCANUM_DIR`) cover the
shipped files.

**Tech Stack:** Rust 2024, `arcanum_formats`, `arcanum_vfs` (dev-dependency for real-data tests),
REA MCP tools on `Arcanum.exe` for research.

**Spec:** `plans/world_milestone.md` (step 1 of "Build order").

## Global Constraints

- `arcanum_formats` has no Bevy dependency and no VFS dependency outside `[dev-dependencies]`.
- Real-data tests skip cleanly (with an `eprintln!("skipping: ...")`) when the game data is
  absent, as in `crates/arcanum_formats/tests/real_files.rs`.
- Synthetic fixtures for every rule; never commit game data.
- REA research runs in Sonnet sub-agents (Agent tool, `model: "sonnet"`), batched by topic,
  returning concise reports (addresses, values, interpretation, confidence). Do not call REA
  tools in the main conversation.
- Every confirmed layout goes in the module's top doc comment and in `NOTES.md`
  ("Format knowledge"), with the Arcanum.exe addresses as evidence.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Work on
  `feat/world-milestone`.

## Known facts (from the data, 2026-11-13)

- Maps live in `modules/Arcanum.dat` under `maps/<name>/` (the loose `modules/Arcanum/maps/`
  files have scrambled names and are not maps; ignore them). Each map folder has `map.prp`,
  `terrain.tdf`, `mapinfo.txt`, `startloc.txt`, `map.jmp`, `<sector id>.sec` and `G_<guid>.mob`.
- `Rules/MapList.mes` 5000: `{Arcanum1-024-fixed, 92958,82592, Type: START_MAP, WorldMap: 0}`.
  Other entries: `{5011}{Bates Mansion Lev 1, 104, 92, WorldMap: 0, Area: 21}`,
  `{5013}{ShopMap, 31, 29, Type: SHOPPING_MAP}`, `{5007}{Void, 1055, 1055}`.
- Sector id = `(sector_y << 26) | sector_x`, sector = 64×64 tiles (`Bates Mansion Lev 1` has
  `0, 1, 2, 67108864, 67108865, …, 134217730`). The start tile (92958, 82592) is in sector
  (1452, 1290), id `1290 * 2^26 + 1452 = 86570436012`; that file is 47291 bytes. All 8
  neighbours exist.
- `Arcanum1-024-fixed/map.prp` (24 bytes): `02000000 01000000 00f40100 00000000 00f40100 00000000`
  → u32 2, u32 1, u64 128000, u64 128000 (width/height in tiles = 2000 sectors).
- `Arcanum1-024-fixed` has 621 `.sec` and 7399 `.mob` files; sectors without a file use the base
  terrain from `terrain.tdf` (out of scope here, see Task 1 notes).
- A sector starts with `u32` light count, then 4096 `u32` tile values (row-major, 64×64): an empty
  sector is 16440 bytes; in the start sector the tiles end at 0x4004 and the rest is lists and
  objects.

## Review Focus

1. A sector whose object list contains a type we have no field table for: parsing must fail with
   a typed error naming the type, never misread the following bytes. (Task 3 test.)
2. Sector files from maps other than the start map (interiors like `Bates Mansion Lev 1`, which
   have different list contents): all must parse to the byte, not only the start map. (Task 2.)
3. Map names with spaces and mixed case in `MapList.mes` (`Bates Mansion Lev 1`) and the VFS's
   case-insensitive lookup: the map list must keep the name exactly. (Task 1 test.)
4. Negative or out-of-range locations (a tile beyond `map.prp` width): location → sector must
   return `None`, not wrap. (Task 1 test.)
5. Art ids whose type has no index file entry (missing scenery art): resolve returns `None`
   and the real-data test reports how many, rather than panicking. (Task 4 test.)

---

### Task 1: Map list, map properties, sector coordinates

**Files:**
- Create: `crates/arcanum_formats/src/map.rs`
- Modify: `crates/arcanum_formats/src/lib.rs` (add `pub mod map;`)
- Test: unit tests in `map.rs`; real-data tests in `crates/arcanum_formats/tests/real_maps.rs`

**Interfaces:**
- Produces:
  - `pub const SECTOR_TILES: u32 = 64;`
  - `pub struct SectorId(pub u64)` with `fn new(x: u32, y: u32) -> Self`, `fn x(self) -> u32`,
    `fn y(self) -> u32`, `fn file_name(self) -> String` (`"<id>.sec"`).
  - `pub struct TileLoc { pub x: u32, pub y: u32 }` with
    `fn sector(self) -> SectorId` and `fn tile_in_sector(self) -> (u32, u32)`.
  - `pub enum MapKind { Normal, Start, Shopping }`
  - `pub struct MapEntry { pub id: u32, pub name: String, pub start: TileLoc, pub kind: MapKind,
    pub world_map: Option<i32>, pub area: Option<i32> }`
  - `pub fn parse_map_list(mes: &MesFile) -> Result<Vec<MapEntry>, MapError>` and
    `pub fn start_map(list: &[MapEntry]) -> Option<&MapEntry>`.
  - `pub struct MapProperties { pub base_terrain: u32, pub unknown: u32, pub width: u64,
    pub height: u64 }` with `fn parse(data: &[u8]) -> Result<Self, MapError>` and
    `fn contains(&self, loc: TileLoc) -> bool`.
  - `pub enum MapError { Truncated { offset: usize, what: &'static str }, BadEntry { id: u32, text: String } }`
  - `pub const MAP_LIST: &str = "Rules/MapList.mes";`

- [ ] **Step 1: Write failing unit tests in `map.rs`**

```rust
#[test] fn sector_id_packs_y_high() {
    let id = SectorId::new(1452, 1290);
    assert_eq!(id.0, 86_570_436_012);
    assert_eq!((id.x(), id.y()), (1452, 1290));
    assert_eq!(SectorId(67_108_865).file_name(), "67108865.sec");
}
#[test] fn tile_to_sector() {
    let t = TileLoc { x: 92958, y: 82592 };
    assert_eq!(t.sector(), SectorId::new(1452, 1290));
    assert_eq!(t.tile_in_sector(), (30, 32));
}
#[test] fn map_list_entries() {
    // synthetic MesFile with 5000, 5011, 5013 from "Known facts" plus a // comment block
    // assert: 5000 name "Arcanum1-024-fixed", start (92958, 82592), kind Start, world_map Some(0), area None
    // assert: 5011 name "Bates Mansion Lev 1" (spaces kept), start (104, 92), area Some(21)
    // assert: 5013 kind Shopping; start_map() returns 5000
}
#[test] fn bad_map_entry_is_an_error() { /* "{5099}{Broken, abc, 3}" -> MapError::BadEntry { id: 5099, .. } */ }
#[test] fn map_properties() {
    let p = MapProperties::parse(&hex("02000000 01000000 00f40100 00000000 00f40100 00000000")).unwrap();
    assert_eq!((p.base_terrain, p.width, p.height), (2, 128_000, 128_000));
    assert!(p.contains(TileLoc { x: 127_999, y: 0 }));
    assert!(!p.contains(TileLoc { x: 128_000, y: 0 }));
    assert!(matches!(MapProperties::parse(&[0; 23]), Err(MapError::Truncated { .. })));
}
```

- [ ] **Step 2: Run `cargo test -p arcanum_formats map::` and confirm the tests fail to compile.**

- [ ] **Step 3: Implement `map.rs`.** Map list entries: split the text on `,`, trim; the first
  three are name, x, y; the rest are `Key: value` pairs (`Type`, `WorldMap`, `Area`); unknown
  keys are ignored. `MapProperties::parse` reads u32, u32, u64, u64 little-endian; the second
  u32's meaning is unknown (name it `unknown` and say so in the doc comment).

- [ ] **Step 4: Write `tests/real_maps.rs`** with a helper that builds a `Vfs` from
  `<game>/modules/Arcanum.PATCH0` (if present), `<game>/modules/Arcanum.dat`, then the game
  folder's `.dat` files, and tests:
  - `start_map_from_map_list`: `start_map()` is `Arcanum1-024-fixed` at (92958, 82592).
  - `start_map_properties`: `maps/Arcanum1-024-fixed/map.prp` parses to width 128000, and
    `contains(start)` is true.
  - `every_map_list_entry_has_a_folder`: every entry's `maps/<name>/map.prp` exists and parses;
    print and allow entries without a folder only if their name is `Void` or `Network Test`
    (record the actual exceptions you find in the test, with a comment).

- [ ] **Step 5: Run `cargo test -p arcanum_formats` and confirm everything passes.**

- [ ] **Step 6: Commit** `map.rs`, `lib.rs`, `tests/real_maps.rs`:
  "Read the map list, map properties and sector coordinates".

### Task 2: Sector files

**Files:**
- Create: `crates/arcanum_formats/src/sector.rs`
- Modify: `crates/arcanum_formats/src/lib.rs`, `crates/arcanum_formats/src/test_util.rs`
  (add `SectorBuilder`), `crates/arcanum_formats/tests/real_maps.rs`, `NOTES.md`

**Interfaces:**
- Consumes: `map::SECTOR_TILES`.
- Produces:
  - `pub struct Tile(pub u32)` with accessors for the fields the research finds (at least
    `fn art_id(self) -> ArtId` once Task 4 exists; until then the raw value).
  - `pub struct Sector { pub lights: Vec<…>, pub tiles: Box<[Tile; 4096]>, pub roofs: …,
    pub blocked: Box<[u32; 128]> /* 4096-bit mask, bit i = tile i */, pub objects: Vec<RawObject>, … }`
    with `fn parse(data: &[u8]) -> Result<Self, SectorError>`, `fn tile(&self, x: u32, y: u32) -> Tile`,
    `fn is_blocked(&self, x: u32, y: u32) -> bool`.
  - `pub struct RawObject { pub offset: usize, pub bytes: Vec<u8> }` (Task 3 turns these into
    `Object`s; this task only needs their extents).
  - `pub enum SectorError { Truncated { offset: usize, what: &'static str }, TrailingBytes { offset: usize, len: usize }, … }`

- [ ] **Step 1: Research (Sonnet sub-agent, REA on Arcanum.exe + the data).** Find the sector
  load function (search strings `.sec`, `sector`, the `"%s\\%I64u.sec"`-style format) and report
  the exact order and encoding of every block after the 4096 tiles: lights, roofs, tile scripts,
  sector script, town map info, aptitude, light scheme, sound list, block mask, objects, and any
  version or terminator values. Also the tile `u32` bit layout (art index, flip, flags). Give the
  agent the start sector's offsets: the tile array ends at 0x4004, and 0x4000.. starts
  `c0010000 01000000 0400aa00 08000000 …`. Ask for confidence per block.

- [ ] **Step 2: Write failing tests.** In `sector.rs`, for each block the research confirms, a
  synthetic test built with `SectorBuilder` (e.g. `empty_sector_is_16440_bytes_and_parses`,
  `tiles_are_row_major`, `blocked_bit_matches_tile`, `object_extents_are_found`,
  `trailing_bytes_are_an_error`, `truncated_block_names_the_block`). In `real_maps.rs`:
  - `start_sector_parses_to_the_byte`: `86570436012.sec` (47291 bytes) parses, no trailing bytes.
  - `start_map_sectors_parse`: all 621 `.sec` in `Arcanum1-024-fixed` parse.
  - `every_sector_of_every_map_parses`: every `.sec` under every `maps/*` folder parses; on
    failure, report the file and the error.

- [ ] **Step 3: Run `cargo test -p arcanum_formats sector` and confirm the tests fail.**

- [ ] **Step 4: Implement `sector.rs` and `SectorBuilder`** to the researched layout. Object
  extents: if the object list stores no lengths, delimit each object by walking it with the
  object reader's field tables; then this task depends on Task 3. In that case, do Task 3's
  research and table work first and keep this task's commit for after Task 3.

- [ ] **Step 5: Run `cargo test -p arcanum_formats` and confirm everything passes.**

- [ ] **Step 6: Update `NOTES.md` "Format knowledge" with the sector layout and addresses, then
  commit:** "Read sector files".

### Task 3: All object types in sector object lists

**Files:**
- Modify: `crates/arcanum_formats/src/object.rs`, `tools/object_probe/fields.py` (if the type
  table is generated there), `crates/arcanum_formats/tests/real_maps.rs`, `NOTES.md`

**Interfaces:**
- Consumes: `Object::parse_instance`, `FieldDef`, `Storage`, `COMMON_FIELDS` (existing).
- Produces:
  - Type constants for every object type (`TYPE_WALL`, `TYPE_PORTAL`, `TYPE_CONTAINER`,
    `TYPE_SCENERY`, `TYPE_PROJECTILE`, weapon … key ring item types, `TYPE_TRAP`, `TYPE_PC = 15`,
    `TYPE_NPC`), numbered as the engine numbers them.
  - Field tables per type block (wall, portal, container, scenery, projectile, item, each item
    subtype, critter, pc, npc, trap) and the bitmap word count per type.
  - `Object::parse_instance` accepts every type; `ObjectError::UnsupportedType(u32)` remains for
    unknown type numbers.
  - `pub fn parse_instance_prefix(data: &[u8]) -> Result<(Object, usize), ObjectError>`: parses
    one object from the start of `data` and returns the bytes consumed (for sector lists).
  - `Object::art_id(&self) -> Option<u32>` (the `current_aid` field).

- [ ] **Step 1: Research (Sonnet sub-agent).** From `tools/object_probe/obj_field_names_reversed.txt`
  and Arcanum.exe: the `*_begin/*_end` blocks for every type, the engine's type enum, which blocks
  each type has, bitmap word counts, and the storage-kind exceptions (as found for PCs:
  `critter_arrows` etc. are object references). Ask it to check its table by walking the objects
  in the start sector and in `Bates Mansion Lev 1`'s 9 sectors with a Python probe (in
  `tools/object_probe/`) to the exact byte.

- [ ] **Step 2: Write failing tests.** Unit: `parse_prefix_reports_consumed_bytes`,
  `wall_instance_round_trip` (and one per type block, from synthetic bytes),
  `unknown_type_is_rejected` (type 99 → `UnsupportedType(99)`). Real-data:
  `start_map_sector_objects_parse`: every object in all 621 start-map sectors parses, and the
  test prints a count per type; `every_sector_object_parses` over every map.

- [ ] **Step 3: Run the tests and confirm they fail.**

- [ ] **Step 4: Implement the tables and `parse_instance_prefix`;** wire `Sector::parse` to use
  it for object extents (and fill `Sector::objects` with parsed `Object`s if that is simpler than
  `RawObject`; update Task 2's interface note in the doc comment if so).

- [ ] **Step 5: Run `cargo test -p arcanum_formats` and confirm everything passes**, including the
  existing 13-prototype and 12-`.mpc` tests.

- [ ] **Step 6: Update `NOTES.md`, then commit:** "Read every object type in sector lists".

### Task 4: Art id → `.art` path for tiles, walls, portals and scenery

**Files:**
- Create: `crates/arcanum_formats/src/art_id.rs`
- Modify: `crates/arcanum_formats/src/lib.rs`, `crates/arcanum_formats/tests/real_maps.rs`,
  `NOTES.md`

**Interfaces:**
- Consumes: `MesFile`, `Sector`, `Tile`, `Object::art_id`.
- Produces:
  - `pub struct ArtId(pub u32)` with `fn kind(self) -> ArtKind` and per-kind accessors (index,
    palette, frame/rotation, flags) as the research finds.
  - `pub enum ArtKind { Tile, Wall, Portal, Scenery, Critter, Item, Interface, … }`
  - `pub struct ArtIndex` built from the index `.mes` files:
    `fn load(read: impl Fn(&str) -> Option<Vec<u8>>) -> Result<Self, ArtIndexError>` and
    `fn path(&self, id: ArtId) -> Option<String>` (a VFS path such as `art/tile/…​.art`).
  - `pub const ART_INDEX_FILES: &[&str]` listing the `.mes` files it reads (e.g.
    `art/tile/tilename.mes`, `art/wall/structure.mes`, `art/portal/portal.mes`,
    `art/scenery/scenery.mes`; confirm the real names).

- [ ] **Step 1: Research (Sonnet sub-agent).** The art id bit layout per kind (type bits, index,
  palette, flip/rotation, frame) and how each kind's index maps to a file: which `.mes` file,
  which key (the index, or a computed key for tiles, which combine two terrain types and a
  variant), and the name → path rule. Ask for the Arcanum.exe functions that build the path.

- [ ] **Step 2: Write failing tests.** Unit, with synthetic `.mes` text: `tile_id_to_path`,
  `wall_id_to_path`, `scenery_id_to_path`, `unknown_index_is_none`. Real-data:
  `start_area_art_resolves`: for the 3×3 sectors around the start, every tile art id and every
  wall/portal/scenery object art id resolves to a path that exists in the VFS; the test fails if
  any does not, listing them. `start_map_art_coverage`: over all 621 sectors, print the number
  of ids that do not resolve per kind (expected 0; if real gaps exist in the shipped data,
  record them in the test with a comment instead of failing).

- [ ] **Step 3: Run the tests and confirm they fail.**

- [ ] **Step 4: Implement `art_id.rs`.**

- [ ] **Step 5: Run `cargo test --workspace` and confirm everything passes.**

- [ ] **Step 6: Update `NOTES.md` (art id layout, index files; remove "art id → file name
  mapping" from "Not yet investigated" and step 4 of "Suggested next steps"), then commit:**
  "Map art ids to .art paths for tiles, walls, portals and scenery".

## Out of scope, noted for later steps

- `terrain.tdf` (base terrain for sectors without a `.sec`; zlib data after a 36-byte header):
  needed only when the view reaches a sector with no file. All 9 sectors around the start exist.
- `.mob` files (per-map mobile objects: NPCs, containers, doors that can change): part of the
  "living map" work, except doors that block. Step 4 decides whether portals in `.mob` matter for
  blocking.
- `mapinfo.txt`, `startloc.txt`, `map.jmp` (lighting/sound schemes, editor start, jump points).
- The world map is 128000×128000 tiles (2000×2000 sectors).
