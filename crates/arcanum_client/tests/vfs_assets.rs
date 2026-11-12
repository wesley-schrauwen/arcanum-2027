//! Loads real game art through Bevy's asset server, backed only by the VFS.
//! Skipped when the game data is not present.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use arcanum_client::{ArtAssetLoader, VfsAssetPlugin};
use arcanum_vfs::Vfs;
use bevy::asset::{AssetPlugin, LoadState};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::app::TaskPoolPlugin;

fn game_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/Arcanum")
}

fn app(vfs: Vfs) -> App {
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        VfsAssetPlugin::new(Arc::new(RwLock::new(vfs))),
        AssetPlugin::default(),
    ))
    .init_asset::<Image>()
    .register_asset_loader(ArtAssetLoader);
    app
}

fn wait_for(app: &mut App, handle: &Handle<Image>) -> LoadState {
    for _ in 0..2000 {
        app.update();
        let state = app.world().resource::<AssetServer>().load_state(handle);
        if matches!(state, LoadState::Loaded | LoadState::Failed(_)) {
            return state;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("timed out waiting for asset");
}

#[test]
fn loads_art_from_dat_archive() {
    if !game_dir().join("arcanum1.dat").exists() {
        eprintln!("skipping: game data not found");
        return;
    }
    let mut app = app(Vfs::from_game_dir(game_dir(), None).unwrap());
    let handle: Handle<Image> = app.world().resource::<AssetServer>().load("art/interface/mainmenuback.art");
    assert!(matches!(wait_for(&mut app, &handle), LoadState::Loaded));
    let image = app.world().resource::<Assets<Image>>().get(&handle).unwrap();
    assert_eq!((image.width(), image.height()), (800, 600));
}

#[test]
fn missing_asset_fails_cleanly() {
    let mut app = app(Vfs::new());
    let handle: Handle<Image> = app.world().resource::<AssetServer>().load("art/nope.art");
    assert!(matches!(wait_for(&mut app, &handle), LoadState::Failed(_)));
}

#[test]
fn override_folder_beats_archive() {
    if !game_dir().join("arcanum1.dat").exists() {
        return;
    }
    // A loose, valid 1x1 static .art in the override folder replaces the archive's 800x600 one.
    let over = std::env::temp_dir().join(format!("arcanum_client_override_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&over);
    std::fs::create_dir_all(over.join("art/interface")).unwrap();
    let mut art = vec![0u8; 0x84];
    art[0..4].copy_from_slice(&1u32.to_le_bytes()); // static
    art[8..12].copy_from_slice(&8u32.to_le_bytes());
    art[0x0c..0x10].copy_from_slice(&1u32.to_le_bytes()); // one palette
    art[0x20..0x24].copy_from_slice(&1u32.to_le_bytes()); // one frame
    art.extend(std::iter::repeat_n(7u8, 256 * 4));
    for v in [1u32, 1, 1, 0, 0, 0, 0] {
        art.extend_from_slice(&v.to_le_bytes());
    }
    art.push(1);
    std::fs::write(over.join("art/interface/MainMenuBack.art"), art).unwrap();

    let mut app = app(Vfs::from_game_dir(game_dir(), Some(&over)).unwrap());
    let handle: Handle<Image> = app.world().resource::<AssetServer>().load("art/interface/mainmenuback.art");
    assert!(matches!(wait_for(&mut app, &handle), LoadState::Loaded));
    let image = app.world().resource::<Assets<Image>>().get(&handle).unwrap();
    assert_eq!((image.width(), image.height()), (1, 1));
    let _ = std::fs::remove_dir_all(&over);
}
