use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use arcanum_client::{ArtAssetLoader, VfsAssetPlugin};
use arcanum_vfs::Vfs;
use bevy::prelude::*;

/// Game data location. Override with `ARCANUM_DIR`; `ARCANUM_OVERRIDE` adds a loose-file folder
/// that shadows the `.dat` archives.
fn build_vfs() -> Vfs {
    let game_dir = PathBuf::from(std::env::var("ARCANUM_DIR").unwrap_or_else(|_| "assets/Arcanum".into()));
    let override_dir = std::env::var("ARCANUM_OVERRIDE").ok().map(PathBuf::from);
    Vfs::from_game_dir(&game_dir, override_dir.as_deref())
        .unwrap_or_else(|e| panic!("cannot open game data in {}: {e}", game_dir.display()))
}

fn main() {
    let vfs = Arc::new(RwLock::new(build_vfs()));
    App::new()
        // Must come before DefaultPlugins so the VFS replaces the default asset source.
        .add_plugins(VfsAssetPlugin::new(vfs))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Arcanum Reimplementation (Bevy)".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .register_asset_loader(ArtAssetLoader)
        .add_systems(Startup, setup_scene)
        .run();
}

fn setup_scene(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn(Camera2d);
    commands.spawn(Sprite::from_image(assets.load("art/interface/mainmenuback.art")));
}
