//! Runs against the real game data; skipped when `assets/` is absent.
use std::path::PathBuf;

use arcanum_formats::map::{MAP_LIST, MapProperties, TileLoc, parse_map_list, start_map};
use arcanum_formats::mes::MesFile;
use arcanum_vfs::Vfs;

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
