use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use arcanum_client::main_menu::menu_ready;
use arcanum_client::{ArcanumAssetsPlugin, DisplayPlugin, GameSettings, MainMenuPlugin, VfsAssetPlugin};
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
    let settings = GameSettings::from_env();
    let mut app = App::new();
    app.insert_resource(settings.clone())
        // Must come before DefaultPlugins so the VFS replaces the default asset source.
        .add_plugins(VfsAssetPlugin::new(vfs))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Arcanum Reimplementation (Bevy)".into(),
                resolution: settings.window_resolution(),
                mode: settings.window_mode(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((ArcanumAssetsPlugin, DisplayPlugin, MainMenuPlugin));

    // Debugging aid: `ARCANUM_SCREENSHOT=out.png` saves the window after a moment and exits.
    if let Ok(path) = std::env::var("ARCANUM_SCREENSHOT") {
        app.insert_resource(ScreenshotPath(path))
            .add_systems(Update, screenshot_and_exit.run_if(menu_ready));
    }
    app.run();
}

#[derive(Resource)]
struct ScreenshotPath(String);

/// Waits for the menu to finish loading, captures the window, then exits once the capture has
/// had time to be written.
fn screenshot_and_exit(
    mut commands: Commands,
    path: Res<ScreenshotPath>,
    mut frames_since_ready: Local<Option<u32>>,
    mut exit: MessageWriter<AppExit>,
) {
    let frames = frames_since_ready.get_or_insert(0);
    *frames += 1;
    // A few frames' grace so the background's scale and the UI layout have been applied.
    if *frames == 10 {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.0.clone()));
    } else if *frames == 100 {
        exit.write(AppExit::Success);
    }
}
