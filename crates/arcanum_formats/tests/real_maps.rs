//! Runs against the real game data; skipped when `assets/` is absent.
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use arcanum_formats::art_id::{ArtId, ArtIndex};
use arcanum_formats::map::{MAP_LIST, MapProperties, SectorId, TileLoc, parse_map_list, start_map};
use arcanum_formats::mes::MesFile;
use arcanum_formats::object::Object;
use arcanum_formats::sector::Sector;
use arcanum_vfs::{ArcanumArchive, Vfs};

/// `ARCANUM_DIR`, or `assets/Arcanum` at the workspace root.
fn game_dir() -> PathBuf {
    std::env::var_os("ARCANUM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/Arcanum"))
}

/// PATCH0, then Arcanum.dat, then the game folder's archives (highest priority first).
fn vfs() -> Option<Vfs> {
    let game = game_dir();
    let modules = game.join("modules");
    if !modules.join("Arcanum.dat").is_file() {
        eprintln!("skipping: game data not found");
        return None;
    }
    let mut vfs = Vfs::new();
    let patch = modules.join("Arcanum.PATCH0");
    if patch.is_file() {
        vfs.mount_archive(patch).unwrap();
    }
    vfs.mount_archive(modules.join("Arcanum.dat")).unwrap();
    vfs.mount_archives_in(&game).unwrap();
    Some(vfs)
}

#[test]
fn start_map_from_map_list() {
    let Some(vfs) = vfs() else { return };
    let mes = MesFile::parse(&vfs.read(MAP_LIST).unwrap()).unwrap();
    let list = parse_map_list(&mes).unwrap();
    let start = start_map(&list).unwrap();
    assert_eq!(start.name, "Arcanum1-024-fixed");
    assert_eq!(start.start, TileLoc { x: 92958, y: 82592 });
}

#[test]
fn start_map_properties() {
    let Some(vfs) = vfs() else { return };
    let mes = MesFile::parse(&vfs.read(MAP_LIST).unwrap()).unwrap();
    let list = parse_map_list(&mes).unwrap();
    let start = start_map(&list).unwrap();
    let prp = MapProperties::parse(&vfs.read(&format!("maps/{}/map.prp", start.name)).unwrap()).unwrap();
    assert_eq!(prp.width, 128_000);
    assert!(prp.contains(start.start));
}

#[test]
fn every_map_list_entry_has_a_folder() {
    let Some(vfs) = vfs() else { return };
    let mes = MesFile::parse(&vfs.read(MAP_LIST).unwrap()).unwrap();
    let list = parse_map_list(&mes).unwrap();
    // Entries observed to have no folder in the shipped data.
    let allowed: &[&str] = &["Network Test", "Multi2"];
    let mut missing = Vec::new();
    for e in &list {
        let path = format!("maps/{}/map.prp", e.name);
        if vfs.exists(&path) {
            MapProperties::parse(&vfs.read(&path).unwrap()).unwrap_or_else(|err| panic!("{path}: {err}"));
        } else {
            missing.push(e.name.as_str());
        }
    }
    eprintln!("map list entries without a folder: {missing:?}");
    for m in &missing {
        assert!(allowed.contains(m), "{m} has no maps/<name>/map.prp");
    }
}

const TYPE_NAMES: [&str; 18] = [
    "wall", "portal", "container", "scenery", "projectile", "weapon", "ammo", "armor", "gold", "food", "scroll", "key",
    "key_ring", "written", "generic", "pc", "npc", "trap",
];

/// Object counts by type name.
fn by_name(counts: &BTreeMap<u32, usize>) -> BTreeMap<&'static str, usize> {
    counts.iter().map(|(t, n)| (TYPE_NAMES[*t as usize], *n)).collect()
}

/// `modules/Arcanum.dat` alone (the counts below are for the unpatched archive).
fn module_archive() -> Option<ArcanumArchive> {
    let path = game_dir().join("modules/Arcanum.dat");
    if !path.is_file() {
        eprintln!("skipping: game data not found");
        return None;
    }
    Some(ArcanumArchive::open(path).unwrap())
}

/// Entries of `archive` under `maps/` ending in `ext` (lowercase).
fn map_files(archive: &ArcanumArchive, ext: &str) -> Vec<String> {
    archive
        .entries()
        .iter()
        .filter(|e| !e.is_dir())
        .map(|e| e.path.clone())
        .filter(|p| {
            let p = p.to_ascii_lowercase().replace('\\', "/");
            p.starts_with("maps/") && p.ends_with(ext)
        })
        .collect()
}

/// Every mobile object file (`maps/*/G_*.mob`, one instance object each) parses to the exact
/// byte. Sectors hold only walls, portals, scenery and traps, so this is the real-data check of
/// the container, item and NPC tables.
#[test]
fn every_mob_object_parses() {
    let Some(archive) = module_archive() else { return };
    let files = map_files(&archive, ".mob");
    let mut counts = BTreeMap::new();
    let mut failures = Vec::new();
    for path in &files {
        let name = path.rsplit(['\\', '/']).next().unwrap();
        assert!(name.starts_with("G_"), "{path}");
        match Object::parse_instance(&archive.read(path).unwrap()) {
            Ok(obj) => {
                // The object's GUID is the file name.
                assert!(path.to_ascii_uppercase().contains(&obj.id.guid().unwrap()), "{path}");
                *counts.entry(obj.object_type).or_insert(0) += 1;
            }
            Err(e) => failures.push(format!("{path}: {e}")),
        }
    }
    eprintln!("{} .mob objects: {:?}", files.len(), by_name(&counts));
    assert!(failures.is_empty(), "{} failed, first: {:?}", failures.len(), &failures[..failures.len().min(5)]);
    let expected = [
        ("ammo", 422),
        ("armor", 1437),
        ("container", 915),
        ("food", 2132),
        ("generic", 1095),
        ("gold", 797),
        ("key", 97),
        ("key_ring", 2),
        ("npc", 3792),
        ("scroll", 180),
        ("weapon", 1718),
        ("written", 136),
    ];
    assert_eq!(by_name(&counts), BTreeMap::from(expected));
    assert_eq!(files.len(), 12_723);
}

/// Adds `sector`'s objects to `counts` by type.
fn count_types(sector: &Sector, counts: &mut BTreeMap<u32, usize>) {
    for o in &sector.objects {
        *counts.entry(o.object_type).or_insert(0) += 1;
    }
}

#[test]
fn start_sector_parses_to_the_byte() {
    let Some(vfs) = vfs() else { return };
    let data = vfs.read("maps/Arcanum1-024-fixed/86570436012.sec").unwrap();
    assert_eq!(data.len(), 47291);
    let sector = Sector::parse(&data).unwrap();
    assert!(!sector.objects.is_empty());
    eprintln!("start sector: version {}, {} lights, {} objects", sector.version, sector.lights.len(), sector.objects.len());
}

/// Every object in all 621 start-map sectors parses (through the VFS, so with the patch).
#[test]
fn start_map_sector_objects_parse() {
    let Some(vfs) = vfs() else { return };
    let files: Vec<String> =
        vfs.list_dir("maps/Arcanum1-024-fixed").unwrap().into_iter().filter(|p| p.to_ascii_lowercase().ends_with(".sec")).collect();
    assert_eq!(files.len(), 621);
    let mut counts = BTreeMap::new();
    for path in &files {
        let sector = Sector::parse(&vfs.read(path).unwrap()).unwrap_or_else(|e| panic!("{path}: {e}"));
        count_types(&sector, &mut counts);
    }
    eprintln!("start map sector objects: {:?}", by_name(&counts));
    assert!(counts.keys().all(|t| [0, 1, 3, 17].contains(t)), "only walls, portals, scenery and traps: {counts:?}");
}

/// Every `.sec` of every map in `Arcanum.dat` parses to the exact byte, and every one in
/// `Arcanum.PATCH0` if present.
#[test]
fn every_sector_of_every_map_parses() {
    let Some(archive) = module_archive() else { return };
    let check = |archive: &ArcanumArchive| {
        let files = map_files(archive, ".sec");
        let mut counts = BTreeMap::new();
        let mut failures = Vec::new();
        for path in &files {
            match Sector::parse(&archive.read(path).unwrap()) {
                Ok(sector) => count_types(&sector, &mut counts),
                Err(e) => failures.push(format!("{path}: {e}")),
            }
        }
        assert!(failures.is_empty(), "{} failed, first: {:?}", failures.len(), &failures[..failures.len().min(5)]);
        (files.len(), counts)
    };
    let (files, counts) = check(&archive);
    eprintln!("Arcanum.dat: {files} sectors, objects {:?}", by_name(&counts));
    // 1680 files: the research walk saw 1668 (its extraction missed `Castle S'nel N'fa` and
    // `Roseborough Inn2`, 6 sectors each).
    assert_eq!(files, 1680);
    assert_eq!(by_name(&counts), BTreeMap::from([("portal", 2107), ("scenery", 170_719), ("trap", 2063), ("wall", 89_013)]));
    let patch = game_dir().join("modules/Arcanum.PATCH0");
    if patch.is_file() {
        let (files, counts) = check(&ArcanumArchive::open(patch).unwrap());
        eprintln!("Arcanum.PATCH0: {files} sectors, objects {:?}", by_name(&counts));
    }
}

fn art_index(vfs: &Vfs) -> ArtIndex {
    ArtIndex::load(|p| vfs.read(p).ok()).unwrap_or_else(|e| panic!("art index: {e}"))
}

/// Resolution outcome counts per art kind name.
#[derive(Default)]
struct ArtStats {
    resolved: usize,
    /// `0xFFFFFFFF`.
    no_art: usize,
    /// The index has no path for the id.
    unresolved: BTreeMap<u32, usize>,
    /// The path does not exist in the VFS.
    missing: BTreeMap<String, usize>,
}

fn check_art(vfs: &Vfs, index: &ArtIndex, cache: &mut HashMap<u32, Option<String>>, stats: &mut BTreeMap<String, ArtStats>, id: ArtId) {
    let s = stats.entry(format!("{:?}", id.kind())).or_default();
    if id.is_none() {
        s.no_art += 1;
        return;
    }
    let path = cache.entry(id.0).or_insert_with(|| index.path(id));
    match path {
        None => *s.unresolved.entry(id.0).or_insert(0) += 1,
        Some(p) if vfs.exists(p) => s.resolved += 1,
        Some(p) => *s.missing.entry(p.clone()).or_insert(0) += 1,
    }
}

/// Checks every tile, roof, light and object art id of `sectors`; returns the stats per kind.
fn art_stats(vfs: &Vfs, sectors: &[String]) -> BTreeMap<String, ArtStats> {
    let index = art_index(vfs);
    let (mut cache, mut stats) = (HashMap::new(), BTreeMap::new());
    for path in sectors {
        let sector = Sector::parse(&vfs.read(path).unwrap()).unwrap_or_else(|e| panic!("{path}: {e}"));
        for t in sector.tiles.iter() {
            check_art(vfs, &index, &mut cache, &mut stats, t.art_id());
        }
        // Roof ids of 0xFFFFFFFF are empty cells, not missing art.
        for r in sector.roofs.iter().flat_map(|r| r.iter()).filter(|r| **r != ArtId::NONE.0) {
            check_art(vfs, &index, &mut cache, &mut stats, ArtId(*r));
        }
        for l in &sector.lights {
            check_art(vfs, &index, &mut cache, &mut stats, ArtId(l.art_id));
        }
        // Traps store no art id.
        for id in sector.objects.iter().filter(|o| o.object_type != 17).filter_map(Object::art_id) {
            check_art(vfs, &index, &mut cache, &mut stats, id);
        }
    }
    stats
}

fn report(label: &str, stats: &BTreeMap<String, ArtStats>) -> usize {
    let mut bad = 0;
    for (kind, s) in stats {
        let (u, m): (usize, usize) = (s.unresolved.values().sum(), s.missing.values().sum());
        eprintln!("{label} {kind}: resolved {}, no art {}, unresolved {u}, missing file {m}", s.resolved, s.no_art);
        bad += u + m;
        for (id, n) in s.unresolved.iter().take(5) {
            eprintln!("   unresolved {id:#010x} x{n}");
        }
        for (p, n) in s.missing.iter().take(5) {
            eprintln!("   missing {p} x{n}");
        }
    }
    bad
}

/// The 3x3 sectors around the start sector: every art id resolves to a file that exists.
#[test]
fn start_area_art_resolves() {
    let Some(vfs) = vfs() else { return };
    let centre = SectorId(86_570_436_012);
    let mut sectors = Vec::new();
    for dy in -1i64..=1 {
        for dx in -1i64..=1 {
            let id = SectorId::new((centre.x() as i64 + dx) as u32, (centre.y() as i64 + dy) as u32);
            sectors.push(format!("maps/Arcanum1-024-fixed/{}", id.file_name()));
        }
    }
    let stats = art_stats(&vfs, &sectors);
    assert_eq!(report("start area", &stats), 0);
    assert!(stats["Tile"].resolved > 0 && stats["Facade"].resolved > 0 && stats["Scenery"].resolved > 0);
}

/// All 621 sectors. Observed: 0 unresolved and 0 missing; the only id without art is one scenery
/// object with `current_aid = 0xFFFFFFFF`.
#[test]
fn start_map_art_coverage() {
    let Some(vfs) = vfs() else { return };
    let sectors: Vec<String> =
        vfs.list_dir("maps/Arcanum1-024-fixed").unwrap().into_iter().filter(|p| p.to_ascii_lowercase().ends_with(".sec")).collect();
    assert_eq!(sectors.len(), 621);
    let stats = art_stats(&vfs, &sectors);
    assert_eq!(report("start map", &stats), 0);
    assert_eq!(stats["Tile"].resolved + stats["Facade"].resolved, 621 * 4096);
    assert_eq!(stats.values().map(|s| s.no_art).sum::<usize>(), 1);
}
