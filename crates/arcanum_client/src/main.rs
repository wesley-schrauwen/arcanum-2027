use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use arcanum_client::{ArcanumAssetsPlugin, MainMenuPlugin, VfsAssetPlugin};
use arcanum_vfs::Vfs;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

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
    let mut app = App::new();
    app
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
        .add_plugins((ArcanumAssetsPlugin, MainMenuPlugin));

    // Debugging aid: `ARCANUM_SCREENSHOT=out.png` saves the window after a moment and exits.
    if let Ok(path) = std::env::var("ARCANUM_SCREENSHOT") {
        app.insert_resource(ScreenshotPath(path)).add_systems(Update, screenshot_and_exit);
    }
    app.run();
}

#[derive(Resource)]
struct ScreenshotPath(String);

fn screenshot_and_exit(
    mut commands: Commands,
    path: Res<ScreenshotPath>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;
    if *frame == 90 {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.0.clone()));
    } else if *frame == 180 {
        exit.write(AppExit::Success);
    }
}
