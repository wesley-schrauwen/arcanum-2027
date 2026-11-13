use std::sync::{Arc, RwLock};

use arcanum_client::main_menu::{MenuLabel, FitToBaseline};
use arcanum_client::options::OptionSlot;
use arcanum_client::{
    ArcanumAssetsPlugin, BitmapTextPlugin, ConfigPlugin, DisplayPlugin, FontsPlugin, LoadGamePlugin, LoadingPlugin, MainMenuPlugin, MenuAudioPlugin, ResourcePack,
    OptionsPlugin, ScreensPlugin, VfsAssetPlugin,
};
use arcanum_client::AppState;
use arcanum_client::config;
use arcanum_client::load_game::{LoadGameScreen, sample_saves};
use arcanum_client::loading::{LoadRequest, LoadingPicture};
use arcanum_client::settings::EnvOverrides;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

/// Debugging aid: when set to a file path, saves the window there once the menu has loaded,
/// then exits.
const SCREENSHOT_VAR: &str = "DEBUG_ARCANUM_SCREENSHOT";
/// Debugging aid: `options`, `singleplayer`, `loadgame` or `loading` opens that screen instead of
/// the main menu.
const SCREEN_VAR: &str = "DEBUG_ARCANUM_SCREEN";

fn main() {
    // Game data: `ARCANUM_DIR` (default `assets/Arcanum`), optional `ARCANUM_OVERRIDE` and
    // `ARCANUM_MODULE` (default `Arcanum`).
    let pack = ResourcePack::from_env();
    let vfs = pack
        .open_vfs()
        .unwrap_or_else(|e| panic!("cannot open game data in {}: {e}", pack.game_dir.display()));
    let vfs = Arc::new(RwLock::new(vfs));
    // Defaults, then the saved config file, then ARCANUM_RES / ARCANUM_FULLSCREEN. Resolved before
    // the window exists so it opens at the saved size.
    let (settings, config) = config::load(config::path_from_env(), EnvOverrides::from_env());
    let mut app = App::new();
    app.insert_resource(settings.clone())
        .insert_resource(config)
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
        .add_plugins((
            ArcanumAssetsPlugin,
            BitmapTextPlugin,
            ConfigPlugin,
            DisplayPlugin,
            FontsPlugin,
            LoadGamePlugin,
            LoadingPlugin,
            MenuAudioPlugin,
            MainMenuPlugin,
            OptionsPlugin,
            ScreensPlugin,
        ));

    // Debugging aid: `DEBUG_ARCANUM_SCREEN=options|singleplayer|loadgame|loading` opens that screen.
    let start = match std::env::var(SCREEN_VAR).unwrap_or_default().to_ascii_lowercase().as_str() {
        "options" => Some(AppState::Options),
        "singleplayer" => Some(AppState::SinglePlayer),
        // With example saves, so the list and info display have something to show.
        "loadgame" => {
            app.insert_resource(sample_saves());
            Some(AppState::LoadGame)
        }
        // Stays up for a minute so it can be captured.
        "loading" => {
            app.insert_resource(LoadRequest::new(AppState::MainMenu).min_secs(60.0));
            Some(AppState::Loading)
        }
        _ => None,
    };
    if let Some(start) = start {
        app.add_systems(Startup, move |mut next: ResMut<NextState<AppState>>| next.set(start));
    }

    if let Ok(path) = std::env::var(SCREENSHOT_VAR) {
        app.insert_resource(ScreenshotPath(path))
            .add_systems(Update, screenshot_and_exit.run_if(screen_ready));
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
    // Grace frames so the layout has been applied and the renderer has finished preparing its
    // pipelines (a screen that is ready very early would otherwise be captured black).
    if *frames == 60 {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.0.clone()));
    } else if *frames == 150 {
        exit.write(AppExit::Success);
    }
}

/// True once a menu, the Options screen or the loading screen is up and the background has been
/// fitted.
fn screen_ready(
    labels: Query<(), With<MenuLabel>>,
    slots: Query<(), With<OptionSlot>>,
    pending: Query<(), With<FitToBaseline>>,
    loading: Query<&ImageNode, With<LoadingPicture>>,
    load_game: Query<(), With<LoadGameScreen>>,
    images: Res<Assets<Image>>,
) -> bool {
    let loading_up = loading.iter().any(|p| images.get(&p.image).is_some());
    (!labels.is_empty() || !slots.is_empty() || loading_up || !load_game.is_empty()) && pending.is_empty()
}
