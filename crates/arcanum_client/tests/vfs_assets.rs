//! Loads real game assets through Bevy's asset server, backed only by the VFS.
//!
//! The game folder comes from the resource pack: `ARCANUM_DIR` if set, otherwise
//! `assets/Arcanum` at the workspace root. Tests skip when no game data is there.
//! `tests/headless.rs` covers the same code with synthetic data.

use std::sync::{Arc, RwLock};

use arcanum_client::resource_pack::{DEFAULT_GAME_DIR, OVERRIDE_DIR_VAR, paths};
use arcanum_client::{ArcanumAssetsPlugin, MesAsset, ResourcePack, VfsAssetPlugin};
use arcanum_formats::test_util::{ArtBuilder, TestFrame};
use arcanum_vfs::Vfs;
use bevy::app::TaskPoolPlugin;
use bevy::asset::{AssetPlugin, LoadState};
use bevy::image::Image;
use bevy::prelude::*;

/// The resource pack from the environment, with the default game folder resolved against the
/// workspace root instead of the test's working directory.
fn pack() -> ResourcePack {
    let workspace_default = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    ResourcePack::from_vars(
        // The override folder is supplied per test, never inherited.
        |name| (name != OVERRIDE_DIR_VAR).then(|| std::env::var(name).ok()).flatten(),
        std::path::Path::new(workspace_default).join(DEFAULT_GAME_DIR),
    )
}

/// The game's VFS, or `None` (and a note) when the game data is not installed.
fn game_vfs(pack: &ResourcePack) -> Option<Vfs> {
    match pack.open_vfs() {
        Ok(vfs) if vfs.exists(paths::backgrounds::MAIN_MENU) => Some(vfs),
        _ => {
            eprintln!("skipping: no game data in {} (set ARCANUM_DIR)", pack.game_dir.display());
            None
        }
    }
}

fn app(vfs: Vfs) -> App {
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        VfsAssetPlugin::new(Arc::new(RwLock::new(vfs))),
        AssetPlugin::default(),
    ))
    .init_asset::<Image>()
    .add_plugins(ArcanumAssetsPlugin);
    app
}

fn load<A: Asset>(app: &mut App, path: &'static str) -> Handle<A> {
    let handle: Handle<A> = app.world().resource::<AssetServer>().load(path);
    for _ in 0..2000 {
        app.update();
        match app.world().resource::<AssetServer>().load_state(&handle) {
            LoadState::Loaded => return handle,
            LoadState::Failed(e) => panic!("{path} failed to load: {e}"),
            _ => std::thread::sleep(std::time::Duration::from_millis(2)),
        }
    }
    panic!("timed out loading {path}");
}

#[test]
fn loads_main_menu_background() {
    let Some(vfs) = game_vfs(&pack()) else { return };
    let mut app = app(vfs);
    let handle = load::<Image>(&mut app, paths::backgrounds::MAIN_MENU);
    let image = app.world().resource::<Assets<Image>>().get(&handle).unwrap();
    assert_eq!((image.width(), image.height()), (800, 600));
}

#[test]
fn loads_main_menu_strings() {
    let Some(vfs) = game_vfs(&pack()) else { return };
    let mut app = app(vfs);
    let handle = load::<MesAsset>(&mut app, paths::text::MAIN_MENU);
    let mes = app.world().resource::<Assets<MesAsset>>().get(&handle).unwrap();
    for (id, text) in [(10, "Single Player"), (11, "MultiPlayer"), (12, "Options"), (13, "Credits"), (14, "Exit Game")] {
        assert_eq!(mes.get(id), Some(text));
    }
}

#[test]
fn override_folder_beats_the_game_archives() {
    let over = tempfile::tempdir().unwrap();
    let pack = ResourcePack { override_dir: Some(over.path().to_path_buf()), ..pack() };
    if game_vfs(&ResourcePack { override_dir: None, ..pack.clone() }).is_none() {
        return;
    }

    // A 1x1 sprite in the override folder replaces the archive's 800x600 background.
    let target = over.path().join(paths::backgrounds::MAIN_MENU);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, ArtBuilder::single(TestFrame::raw(1, 1, &[1])).build()).unwrap();

    let mut app = app(pack.open_vfs().unwrap());
    let handle = load::<Image>(&mut app, paths::backgrounds::MAIN_MENU);
    let image = app.world().resource::<Assets<Image>>().get(&handle).unwrap();
    assert_eq!((image.width(), image.height()), (1, 1));
}

#[test]
fn every_resource_pack_path_exists_in_the_game_data() {
    let Some(vfs) = game_vfs(&pack()) else { return };
    for file in paths::ALL_FILES {
        assert!(vfs.exists(file), "missing file {file}");
    }
    for dir in paths::ALL_DIRS {
        assert!(vfs.is_dir(dir), "missing directory {dir}");
        assert!(!vfs.list_dir(dir).unwrap().is_empty(), "empty directory {dir}");
    }
}

#[test]
fn pre_made_characters_load_with_their_story_and_portrait() {
    let Some(vfs) = game_vfs(&pack()) else { return };
    let premades = arcanum_client::premade::load(&vfs).unwrap();
    assert_eq!(premades.len(), 12);
    let lugard = premades.iter().find(|p| p.name == "Lugard Bloodstone").unwrap();
    assert_eq!((lugard.race.as_str(), lugard.gender.as_str()), ("Dwarf", "Male"));
    assert_eq!(lugard.background, 46);
    assert_eq!(arcanum_client::premade::split_story(&lugard.story).0, "Lugard Bloodstone");
    assert!(lugard.story.contains("steam-powered shovel"));
    assert_eq!(lugard.portrait.as_deref(), Some("portrait/DWM2_b.bmp"));
    for p in &premades {
        assert!(!p.name.is_empty() && !p.story.is_empty() && !p.race.is_empty(), "{p:?}");
        assert!(vfs.exists(p.portrait.as_deref().unwrap()), "{p:?}");
    }
    // The prototypes' real names, not the .mpc placeholders.
    assert!(premades.iter().any(|p| p.name == "Clarisse Vorak"));
}
