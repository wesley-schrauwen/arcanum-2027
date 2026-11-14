//! Runs against the real game data; skipped when `assets/` is absent.
use std::collections::BTreeMap;
use std::path::PathBuf;

use arcanum_formats::map::{MAP_LIST, MapProperties, TileLoc, parse_map_list, start_map};
use arcanum_formats::mes::MesFile;
use arcanum_formats::object::Object;
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
