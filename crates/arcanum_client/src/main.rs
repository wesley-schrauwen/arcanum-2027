use std::sync::{Arc, RwLock};

use arcanum_client::main_menu::menu_ready;
use arcanum_client::{
    ArcanumAssetsPlugin, DisplayPlugin, FontsPlugin, GameSettings, MainMenuPlugin, MenuAudioPlugin, ResourcePack,
    ScreensPlugin, VfsAssetPlugin,
};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

/// Debugging aid: when set to a file path, saves the window there once the menu has loaded,
/// then exits.
const SCREENSHOT_VAR: &str = "DEBUG_ARCANUM_SCREENSHOT";

fn main() {
    // Game data: `ARCANUM_DIR` (default `assets/Arcanum`), optional `ARCANUM_OVERRIDE` and
    // `ARCANUM_MODULE` (default `Arcanum`).
    let pack = ResourcePack::from_env();
    let vfs = pack
        .open_vfs()
        .unwrap_or_else(|e| panic!("cannot open game data in {}: {e}", pack.game_dir.display()));
    let vfs = Arc::new(RwLock::new(vfs));
    let settings = GameSettings::from_env();
    let mut app = App::new();
    app.insert_resource(settings.clone())
        .insert_resource(pack)
        // Must come before DefaultPlugins so the VFS replaces the default asset source.
        .add_plugins(VfsAssetPlugin::new(vfs))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Open Arcanum (2026-November)".into(),
                resolution: settings.window_resolution(),
                mode: settings.window_mode(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((ArcanumAssetsPlugin, DisplayPlugin, FontsPlugin, MenuAudioPlugin, MainMenuPlugin, ScreensPlugin));

    if let Ok(path) = std::env::var(SCREENSHOT_VAR) {
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
