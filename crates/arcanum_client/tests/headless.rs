//! Headless tests: no window, no GPU and no game data. Every asset comes from synthetic
//! `.dat` archives and override folders built in a temp directory.

use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use arcanum_client::audio::{MENU_MUSIC_VOLUME, MenuMusic, MenuSounds, SoundEffect, VoiceLine, play_voice};
use arcanum_client::config::{self, ConfigPlugin};
use arcanum_client::settings::EnvOverrides;
use arcanum_client::bitmap_text::{BitmapFontAsset, BitmapText, BitmapTextRendered, compose_rgba};
use arcanum_client::fonts::{ArcanumFonts, ORIGINAL_SCALE};
use arcanum_client::main_menu::{
    DISABLED_COLOR, HOVER_COLOR, MENU_ENTRY_IDS, MenuAction, MenuLabel, OnMainMenu, TEXT_COLOR, menu_ready,
};
use arcanum_client::options::{
    CurrentTab, KEEP_RESOLUTION_TITLE, OptionId, OptionSlot, OptionValue, OptionsAction, OptionsScreen, OptionsTab, ResolutionConfirm,
    ResolutionCountdown, ResolutionDialog, SliderFill,
};
use arcanum_client::loading::{LoadRequest, LoadingIndicator, LoadingPicture, LoadingProgress, LoadingScreen, MIN_LOADING_SECS};
use arcanum_client::LoadingPlugin;
use arcanum_client::LoadGamePlugin;
use arcanum_client::NewGamePlugin;
use arcanum_client::new_game::{NewGameAction, NewGameScreen};
use arcanum_client::load_game::{
    HoverHelpText, LoadGameAction, LoadGameScreen, LoadGameState, NO_SAVES, SaveGames, SaveInfo, SaveRow, sample_saves,
};
use arcanum_client::screens::{PlaceholderScreen, RETURN_HINT};
use arcanum_client::widgets::{FrameButton, Selected};
use arcanum_client::resource_pack::paths;
use arcanum_client::settings::BASELINE_HEIGHT;
use arcanum_client::{
    AppState, ArcanumAssetsPlugin, ArtFrames, BitmapTextPlugin, DisplayPlugin, OptionsPlugin, DisplayResolution, FontsPlugin, GameSettings, MainMenuPlugin,
    MenuAudioPlugin, MesAsset, ScreensPlugin, VfsAssetPlugin, VfsAssetReader,
};
use arcanum_formats::font::BitmapFont;
use arcanum_formats::test_util::{ArtBuilder, TestFrame, font_art, ramp_palette};
use arcanum_vfs::Vfs;
use arcanum_vfs::test_util::DatBuilder;
use bevy::app::TaskPoolPlugin;
use bevy::asset::io::{AssetReader, AssetReaderError, Reader};
use bevy::asset::{AssetPlugin, LoadState};
use bevy::audio::{AudioSource, PlaybackMode, Volume};
use bevy::camera::ScalingMode;
use bevy::ecs::system::RunSystemOnce;
use bevy::picking::hover::Hovered;
use bevy::state::app::StatesPlugin;
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::ui::Pressed;
use bevy::text::FontSource;
use bevy::ui_widgets::{Activate, ActivateOnPress, Button};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::tasks::block_on;
use bevy::tasks::futures_lite::StreamExt;
use bevy::window::{MonitorSelection, WindowMode, WindowResolution};

// ---------------------------------------------------------------------------------------------
// Fixtures

const MENU_MES: &[u8] = b"// test menu\r\n{10}{Play}\r\n{11}{Online}\r\n{12}{Settings}\r\n{13}{Thanks}\r\n{14}{Quit}\r\n{34}{Onward}{50}{Begin}{51}{Resume}{52}{Latest}{53}{Movie}{54}{Back}\r\n{4000}{Gameplay}{4001}{Picture}{4002}{Audio}{4003}{Finished}\r\n";

/// An 8x6 static sprite whose pixel at index `i` uses palette index `i % 256`.
fn menu_art_indices() -> Vec<u8> {
    (0..48u8).collect()
}

fn menu_art() -> Vec<u8> {
    ArtBuilder::single(TestFrame::raw(8, 6, &menu_art_indices())).build()
}

fn expected_rgba(indices: &[u8]) -> Vec<u8> {
    let pal = ramp_palette();
    indices
        .iter()
        .flat_map(|&i| {
            let [b, g, r, _] = pal[i as usize];
            [r, g, b, if i == 0 { 0 } else { 255 }]
        })
        .collect()
}

/// A temp game directory with one archive holding the menu art (zlib) and strings (raw).
struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    /// Stored with `\` separators, as in the real archives; looked up via `paths::*`.
    fn new() -> Self {
        Self::with_archive(
            DatBuilder::new()
                .zlib(&paths::backgrounds::MAIN_MENU.replace('/', "\\"), &menu_art())
                .raw(&paths::text::MAIN_MENU.replace('/', "\\"), MENU_MES),
        )
    }

    fn with_archive(archive: DatBuilder) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("game")).unwrap();
        archive.write(dir.path().join("game/arcanum1.dat"));
        Self { dir }
    }

    /// Writes a loose file into the override folder.
    fn override_file(&self, rel: &str, data: &[u8]) {
        let p = self.dir.path().join("override").join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }

    fn vfs(&self) -> Vfs {
        let over = self.dir.path().join("override");
        Vfs::from_game_dir(self.dir.path().join("game"), over.exists().then_some(over.as_path())).unwrap()
    }
}

/// An app with the VFS as its only asset source, the Arcanum loaders and nothing that needs a
/// window or GPU.
fn asset_app(vfs: Vfs) -> App {
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

/// Runs `app` until `done` returns true, failing after a few seconds.
fn update_until(app: &mut App, mut done: impl FnMut(&mut App) -> bool) {
    for _ in 0..2000 {
        app.update();
        if done(app) {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("condition not reached");
}

fn load<A: Asset>(app: &mut App, path: &'static str) -> (Handle<A>, LoadState) {
    let handle: Handle<A> = app.world().resource::<AssetServer>().load(path);
    let mut state = LoadState::NotLoaded;
    update_until(app, |app| {
        state = app.world().resource::<AssetServer>().load_state(&handle);
        matches!(state, LoadState::Loaded | LoadState::Failed(_))
    });
    (handle, state)
}

// ---------------------------------------------------------------------------------------------
// VfsAssetReader

fn read_all(reader: &VfsAssetReader, path: &str) -> Result<Vec<u8>, AssetReaderError> {
    block_on(async {
        let mut r = reader.read(Path::new(path)).await?;
        let mut buf = Vec::new();
        r.read_to_end(&mut buf).await?;
        Ok(buf)
    })
}

fn reader(fixture: &Fixture) -> VfsAssetReader {
    VfsAssetReader::new(Arc::new(RwLock::new(fixture.vfs())))
}

#[test]
fn reader_serves_archive_and_override_files() {
    let fx = Fixture::new();
    fx.override_file("Mes/Extra.mes", b"{1}{loose}");
    let r = reader(&fx);

    assert_eq!(read_all(&r, paths::text::MAIN_MENU).unwrap(), MENU_MES);
    assert_eq!(read_all(&r, paths::backgrounds::MAIN_MENU).unwrap(), menu_art());
    assert_eq!(read_all(&r, "mes/extra.mes").unwrap(), b"{1}{loose}");
}

#[test]
fn reader_reports_missing_files_as_not_found() {
    let r = reader(&Fixture::new());
    for path in ["nope.art", "art/interface", "../outside"] {
        match read_all(&r, path) {
            Err(AssetReaderError::NotFound(p)) => assert_eq!(p, Path::new(path)),
            other => panic!("{path}: expected NotFound, got {other:?}"),
        }
    }
}

#[test]
fn reader_has_no_meta_files() {
    let r = reader(&Fixture::new());
    let result = block_on(async { r.read_meta(Path::new(paths::text::MAIN_MENU)).await.map(|_| ()) });
    assert!(matches!(result, Err(AssetReaderError::NotFound(_))));
}

#[test]
fn reader_directories() {
    let fx = Fixture::new();
    fx.override_file("art/interface/extra.art", b"");
    let r = reader(&fx);

    assert!(block_on(r.is_directory(Path::new("art/interface"))).unwrap());
    assert!(block_on(r.is_directory(Path::new(""))).unwrap());
    assert!(!block_on(r.is_directory(Path::new(paths::text::MAIN_MENU))).unwrap());
    assert!(!block_on(r.is_directory(Path::new("missing"))).unwrap());

    let listed: Vec<_> = block_on(async {
        r.read_directory(Path::new("art/interface")).await.unwrap().collect::<Vec<_>>().await
    });
    let listed: Vec<String> = listed.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    assert_eq!(listed, ["art/interface/extra.art", paths::backgrounds::MAIN_MENU]);

    let missing = block_on(async { r.read_directory(Path::new("missing")).await.map(|_| ()) });
    assert!(matches!(missing, Err(AssetReaderError::NotFound(_))));
}

// ---------------------------------------------------------------------------------------------
// Asset loaders

#[test]
fn art_loads_as_an_rgba_image() {
    let mut app = asset_app(Fixture::new().vfs());
    let (handle, state) = load::<Image>(&mut app, paths::backgrounds::MAIN_MENU);
    assert!(matches!(state, LoadState::Loaded));

    let image = app.world().resource::<Assets<Image>>().get(&handle).unwrap();
    assert_eq!((image.width(), image.height()), (8, 6));
    assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
    assert_eq!(image.data.as_deref(), Some(expected_rgba(&menu_art_indices()).as_slice()));
}

#[test]
fn rle_art_loads() {
    let indices: Vec<u8> = [vec![3u8; 30], vec![1, 2], vec![9; 32]].concat();
    let fx = Fixture::with_archive(DatBuilder::new().raw("rle.art", &ArtBuilder::single(TestFrame::rle(8, 8, &indices)).build()));
    let mut app = asset_app(fx.vfs());
    let (handle, state) = load::<Image>(&mut app, "rle.art");
    assert!(matches!(state, LoadState::Loaded));
    let image = app.world().resource::<Assets<Image>>().get(&handle).unwrap();
    assert_eq!(image.data.as_deref(), Some(expected_rgba(&indices).as_slice()));
}

#[test]
fn mes_loads_as_an_asset() {
    let mut app = asset_app(Fixture::new().vfs());
    let (handle, state) = load::<MesAsset>(&mut app, "MES/mainmenu.MES");
    assert!(matches!(state, LoadState::Loaded));
    let mes = app.world().resource::<Assets<MesAsset>>().get(&handle).unwrap();
    assert_eq!(mes.get(10), Some("Play"));
    assert_eq!(mes.get(14), Some("Quit"));
}

#[test]
fn override_folder_beats_archive_for_assets() {
    let fx = Fixture::new();
    fx.override_file(paths::text::MAIN_MENU, b"{10}{Overridden}");
    let mut app = asset_app(fx.vfs());
    let (handle, _) = load::<MesAsset>(&mut app, paths::text::MAIN_MENU);
    assert_eq!(app.world().resource::<Assets<MesAsset>>().get(&handle).unwrap().get(10), Some("Overridden"));
}

#[test]
fn bad_assets_fail_without_panicking() {
    let fx = Fixture::with_archive(
        DatBuilder::new()
            .raw("bad.art", b"not an art file")
            .raw("bad.mes", b"{oops}{x}")
            .raw("unterminated.mes", b"{1}{never closed"),
    );
    let mut app = asset_app(fx.vfs());
    assert!(matches!(load::<Image>(&mut app, "bad.art").1, LoadState::Failed(_)));
    assert!(matches!(load::<Image>(&mut app, "missing.art").1, LoadState::Failed(_)));
    assert!(matches!(load::<MesAsset>(&mut app, "bad.mes").1, LoadState::Failed(_)));
    assert!(matches!(load::<MesAsset>(&mut app, "unterminated.mes").1, LoadState::Failed(_)));
}

// ---------------------------------------------------------------------------------------------
// Main menu

/// The full menu stack minus anything needing a window, GPU or audio device: audio entities
/// are spawned but nothing plays them.
fn menu_app(vfs: Vfs) -> App {
    let mut app = asset_app(vfs);
    app.add_plugins(StatesPlugin)
        .init_asset::<AudioSource>()
        .init_asset::<Font>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<GameSettings>()
        .add_plugins((TimePlugin, BitmapTextPlugin, FontsPlugin, MenuAudioPlugin, MainMenuPlugin, OptionsPlugin, ScreensPlugin, LoadingPlugin, LoadGamePlugin, NewGamePlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)));
    app
}

fn run_until_menu_ready(app: &mut App) {
    update_until(app, |app| {
        app.world_mut().run_system_cached(menu_ready).unwrap()
    });
}

/// Label ids in on-screen order, read from the UI hierarchy.
fn label_order(app: &mut App) -> Vec<(u32, String)> {
    let world = app.world_mut();
    let mut labels = world.query::<(Entity, &MenuLabel, Option<&Text>, Option<&BitmapText>, &ChildOf)>();
    let entries: Vec<(Entity, u32, String, Entity)> = labels
        .iter(world)
        .map(|(e, l, t, b, parent)| {
            let text = t.map(|t| t.0.clone()).or_else(|| b.map(|b| b.text.clone())).unwrap();
            (e, l.0, text, parent.parent())
        })
        .collect();
    if entries.is_empty() {
        return Vec::new();
    }
    // label -> row -> column
    let column = world.get::<ChildOf>(entries[0].3).unwrap().parent();
    let rows = world.get::<Children>(column).unwrap().to_vec();
    rows.iter()
        .map(|row| {
            let (_, id, text, _) = entries.iter().find(|(_, _, _, parent)| parent == row).unwrap();
            (*id, text.clone())
        })
        .collect()
}

#[test]
fn main_menu_spawns_camera_background_and_labels() {
    let mut app = menu_app(Fixture::new().vfs());
    run_until_menu_ready(&mut app);

    // Camera: fixed 1080-unit-tall view.
    let world = app.world_mut();
    let projections: Vec<Projection> =
        world.query_filtered::<&Projection, With<Camera2d>>().iter(world).cloned().collect();
    assert_eq!(projections.len(), 1);
    match &projections[0] {
        Projection::Orthographic(o) => {
            assert!(matches!(o.scaling_mode, ScalingMode::FixedVertical { viewport_height } if viewport_height == BASELINE_HEIGHT));
        }
        other => panic!("unexpected projection {other:?}"),
    }

    // Background: 6 pixels tall, scaled up to the 1080 baseline, aspect ratio kept.
    let scales: Vec<Vec3> = world.query_filtered::<&Transform, With<Sprite>>().iter(world).map(|t| t.scale).collect();
    assert_eq!(scales, vec![Vec3::splat(BASELINE_HEIGHT / 6.0)]);

    assert_eq!(world.resource::<ClearColor>().0, Color::BLACK);

    // Labels: one per entry, in id order, text from the .mes file.
    let expected: Vec<(u32, String)> = MENU_ENTRY_IDS
        .into_iter()
        .zip(["Play", "Settings", "Thanks", "Quit"])
        .map(|(id, t)| (id, t.to_string()))
        .collect();
    assert_eq!(label_order(&mut app), expected);
}

#[test]
fn main_menu_labels_spawn_once() {
    let mut app = menu_app(Fixture::new().vfs());
    run_until_menu_ready(&mut app);
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(label_order(&mut app).len(), MENU_ENTRY_IDS.len());
}

#[test]
fn main_menu_skips_missing_messages() {
    let fx = Fixture::with_archive(
        DatBuilder::new()
            .raw(paths::backgrounds::MAIN_MENU, &menu_art())
            .raw(paths::text::MAIN_MENU, b"{10}{Play}{14}{Quit}"),
    );
    let mut app = menu_app(fx.vfs());
    run_until_menu_ready(&mut app);
    assert_eq!(label_order(&mut app), vec![(10, "Play".into()), (14, "Quit".into())]);
}

#[test]
fn main_menu_survives_missing_strings() {
    let fx = Fixture::with_archive(DatBuilder::new().raw(paths::backgrounds::MAIN_MENU, &menu_art()));
    let mut app = menu_app(fx.vfs());
    // The background still loads and is fitted; no labels appear and nothing panics.
    update_until(&mut app, |app| {
        let world = app.world_mut();
        world.query_filtered::<&Transform, With<Sprite>>().iter(world).any(|t| t.scale != Vec3::ONE)
    });
    for _ in 0..50 {
        app.update();
    }
    assert!(label_order(&mut app).is_empty());
}

#[test]
fn main_menu_survives_missing_background() {
    let fx = Fixture::with_archive(DatBuilder::new().raw(paths::text::MAIN_MENU, MENU_MES));
    let mut app = menu_app(fx.vfs());
    update_until(&mut app, |app| !label_order(app).is_empty());
    assert_eq!(label_order(&mut app).len(), 4);
}

// ---------------------------------------------------------------------------------------------
// Display settings and UI scale

fn display_app(settings: GameSettings, window: WindowResolution) -> App {
    let mut app = App::new();
    app.add_plugins(DisplayPlugin).init_resource::<UiScale>().insert_resource(settings);
    app.world_mut().spawn(Window { resolution: window, ..default() });
    app.update();
    app
}

fn window(app: &mut App) -> Window {
    let world = app.world_mut();
    world.query::<&Window>().single(world).unwrap().clone()
}

fn ui_scale(app: &App) -> f32 {
    app.world().resource::<UiScale>().0
}

fn settings(resolution: DisplayResolution, fullscreen: bool) -> GameSettings {
    GameSettings { resolution, fullscreen, ..default() }
}

#[test]
fn ui_scale_follows_window_height() {
    for (res, scale) in [
        (DisplayResolution::Hd, 720.0 / 1080.0),
        (DisplayResolution::HdPlus, 900.0 / 1080.0),
        (DisplayResolution::Fhd, 1.0),
        (DisplayResolution::Qhd, 1440.0 / 1080.0),
        (DisplayResolution::Wqxga, 1600.0 / 1080.0),
        (DisplayResolution::UltrawideQhd, 1440.0 / 1080.0),
        (DisplayResolution::Uhd4k, 2.0),
    ] {
        let s = settings(res, false);
        let app = display_app(s.clone(), s.window_resolution());
        assert!((ui_scale(&app) - scale).abs() < 1e-4, "{res:?}: {}", ui_scale(&app));
    }
}

#[test]
fn ui_scale_uses_logical_pixels_on_hidpi() {
    // 4K physical on a 2x display is 1080 logical: the same layout as 1080p.
    let s = settings(DisplayResolution::Uhd4k, false);
    let app = display_app(s.clone(), s.window_resolution().with_scale_factor_override(2.0));
    assert_eq!(ui_scale(&app), 1.0);
}

#[test]
fn changing_settings_resizes_the_window() {
    let s = settings(DisplayResolution::Fhd, false);
    let mut app = display_app(s.clone(), s.window_resolution());

    app.world_mut().resource_mut::<GameSettings>().resolution = DisplayResolution::Qhd;
    app.update();
    assert_eq!(window(&mut app).resolution.physical_size(), UVec2::new(2560, 1440));
    assert!((ui_scale(&app) - 1440.0 / 1080.0).abs() < 1e-4);

    app.world_mut().resource_mut::<GameSettings>().resolution = DisplayResolution::Custom(1366, 768);
    app.update();
    assert_eq!(window(&mut app).resolution.physical_size(), UVec2::new(1366, 768));
}

#[test]
fn changing_settings_resizes_hidpi_windows_in_physical_pixels() {
    let s = settings(DisplayResolution::Uhd4k, false);
    let mut app = display_app(s.clone(), s.window_resolution().with_scale_factor_override(2.0));
    app.world_mut().resource_mut::<GameSettings>().resolution = DisplayResolution::Qhd;
    app.update();
    let w = window(&mut app);
    assert_eq!(w.resolution.physical_size(), UVec2::new(2560, 1440));
    assert_eq!(w.resolution.height(), 720.0);
    assert!((ui_scale(&app) - 720.0 / 1080.0).abs() < 1e-4);
}

#[test]
fn unsupported_resolutions_are_not_applied() {
    let s = settings(DisplayResolution::Fhd, false);
    let mut app = display_app(s.clone(), s.window_resolution());
    // `Custom` can be built directly, bypassing validation; the window must not follow it.
    for bad in [DisplayResolution::Custom(800, 600), DisplayResolution::Custom(1280, 1024), DisplayResolution::Custom(9000, 5000)] {
        app.world_mut().resource_mut::<GameSettings>().resolution = bad;
        app.update();
        assert_eq!(window(&mut app).resolution.physical_size(), UVec2::new(1920, 1080), "{bad:?}");
        assert_eq!(ui_scale(&app), 1.0);
    }
}

#[test]
fn changing_settings_toggles_fullscreen() {
    let s = settings(DisplayResolution::Fhd, false);
    let mut app = display_app(s.clone(), s.window_resolution());
    assert_eq!(window(&mut app).mode, WindowMode::Windowed);

    app.world_mut().resource_mut::<GameSettings>().fullscreen = true;
    app.update();
    assert_eq!(window(&mut app).mode, WindowMode::BorderlessFullscreen(MonitorSelection::Primary));

    app.world_mut().resource_mut::<GameSettings>().fullscreen = false;
    app.update();
    assert_eq!(window(&mut app).mode, WindowMode::Windowed);
}

#[test]
fn ui_scale_follows_manual_resizes() {
    let s = settings(DisplayResolution::Fhd, false);
    let mut app = display_app(s.clone(), s.window_resolution());
    {
        let world = app.world_mut();
        let mut q = world.query::<&mut Window>();
        q.single_mut(world).unwrap().resolution.set(1000.0, 540.0);
    }
    app.update();
    assert_eq!(ui_scale(&app), 0.5);
    // A resize alone must not be undone: settings did not change.
    assert_eq!(window(&mut app).resolution.height(), 540.0);
}

#[test]
fn display_plugin_without_a_window_does_nothing() {
    let mut app = App::new();
    app.add_plugins(DisplayPlugin).init_resource::<UiScale>().insert_resource(GameSettings::default());
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(ui_scale(&app), 1.0);
}

// ---------------------------------------------------------------------------------------------
// Menu interaction, screens, audio and fonts

/// Menu buttons (the text entities) in on-screen order.
fn buttons(app: &mut App) -> Vec<(Entity, MenuAction)> {
    let world = app.world_mut();
    let Some(column) = world.query_filtered::<Entity, With<OnMainMenu>>().iter(world).next() else {
        return Vec::new();
    };
    let rows = world.get::<Children>(column).unwrap().to_vec();
    rows.into_iter()
        .map(|row| {
            let label = world.get::<Children>(row).unwrap()[0];
            (label, *world.get::<MenuAction>(label).unwrap())
        })
        .collect()
}

fn button_for(app: &mut App, action: MenuAction) -> Entity {
    buttons(app).into_iter().find(|(_, a)| *a == action).unwrap().0
}

fn label_color(app: &mut App, button: Entity) -> Color {
    app.world().get::<TextColor>(button).unwrap().0
}

fn state(app: &App) -> AppState {
    *app.world().resource::<State<AppState>>().get()
}

fn activate(app: &mut App, action: MenuAction) {
    let entity = button_for(app, action);
    app.world_mut().trigger(Activate { entity });
    app.update();
}

fn press_escape(app: &mut App) {
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
    app.update();
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.release(KeyCode::Escape);
    keys.clear();
    // The state change queued by the key press applies at the start of the next frame.
    app.update();
}

fn ready_menu(fx: &Fixture) -> App {
    let mut app = menu_app(fx.vfs());
    run_until_menu_ready(&mut app);
    app
}

fn texts_under<T: Component>(app: &mut App) -> Vec<String> {
    let world = app.world_mut();
    let roots: Vec<Entity> = world.query_filtered::<Entity, With<T>>().iter(world).collect();
    let mut out = Vec::new();
    for root in roots {
        for child in world.get::<Children>(root).map(|c| c.to_vec()).unwrap_or_default() {
            if let Some(text) = world.get::<Text>(child) {
                out.push(text.0.clone());
            } else if let Some(text) = world.get::<BitmapText>(child) {
                out.push(text.text.clone());
            }
        }
    }
    out
}

#[test]
fn menu_entries_are_buttons_in_order() {
    let mut app = ready_menu(&Fixture::new());
    let actions: Vec<MenuAction> = buttons(&mut app).into_iter().map(|(_, a)| a).collect();
    assert_eq!(
        actions,
        [MenuAction::SinglePlayer, MenuAction::Options, MenuAction::Credits, MenuAction::ExitGame]
    );
    for (button, _) in buttons(&mut app) {
        let world = app.world();
        // The hit area is the text: the button carries the text and is not the full-width row.
        assert!(world.get::<MenuLabel>(button).is_some());
        assert!(world.get::<Text>(button).is_some() || world.get::<BitmapText>(button).is_some());
        let row = world.get::<ChildOf>(button).unwrap().parent();
        assert!(world.get::<Button>(row).is_none() && world.get::<Hovered>(row).is_none());
        assert!(world.get::<Button>(button).is_some());
        assert!(world.get::<ActivateOnPress>(button).is_some());
        assert!(world.get::<Hovered>(button).is_some());
        assert_eq!(label_color(&mut app, button), TEXT_COLOR);
    }
}

#[test]
fn hovering_highlights_only_that_entry() {
    let mut app = ready_menu(&Fixture::new());
    let options = button_for(&mut app, MenuAction::Options);
    app.world_mut().entity_mut(options).insert(Hovered(true));
    app.update();
    for (button, action) in buttons(&mut app) {
        let expected = if action == MenuAction::Options { HOVER_COLOR } else { TEXT_COLOR };
        assert_eq!(label_color(&mut app, button), expected, "{action:?}");
    }

    app.world_mut().entity_mut(options).insert(Hovered(false));
    app.update();
    assert_eq!(label_color(&mut app, options), TEXT_COLOR);
}

#[test]
fn entries_open_their_screen_and_escape_returns() {
    let mut app = ready_menu(&Fixture::new());
    for (action, target, title) in [(MenuAction::Credits, AppState::Credits, "Thanks")] {
        activate(&mut app, action);
        assert_eq!(state(&app), target);
        // The main menu's buttons are gone; the placeholder shows its title from MainMenu.mes.
        assert!(buttons(&mut app).is_empty(), "{action:?}");
        assert!(label_order(&mut app).is_empty());
        assert_eq!(texts_under::<PlaceholderScreen>(&mut app), [title, RETURN_HINT]);
        // The background stays.
        let world = app.world_mut();
        assert_eq!(world.query::<&Sprite>().iter(world).count(), 1);

        press_escape(&mut app);
        assert_eq!(state(&app), AppState::MainMenu);
        assert!(texts_under::<PlaceholderScreen>(&mut app).is_empty());
        assert_eq!(label_order(&mut app).len(), 4, "menu rebuilt after {action:?}");
    }
}

#[test]
fn escape_on_the_main_menu_does_nothing() {
    let mut app = ready_menu(&Fixture::new());
    press_escape(&mut app);
    assert_eq!(state(&app), AppState::MainMenu);
    assert_eq!(buttons(&mut app).len(), 4);
}

#[test]
fn exit_game_requests_app_exit() {
    let mut app = ready_menu(&Fixture::new());
    assert!(app.should_exit().is_none());
    activate(&mut app, MenuAction::ExitGame);
    assert_eq!(app.should_exit(), Some(AppExit::Success));
}

#[test]
fn there_is_no_multiplayer_entry() {
    let mut app = ready_menu(&Fixture::new());
    let texts: Vec<String> = label_order(&mut app).into_iter().map(|(_, t)| t).collect();
    assert!(!texts.iter().any(|t| t == "Online"), "{texts:?}");
    assert!(!label_order(&mut app).iter().any(|(id, _)| *id == 11));
}

// ---------------------------------------------------------------------------------------------
// Single Player menu

fn open_single_player(fx: &Fixture) -> App {
    let mut app = ready_menu(fx);
    activate(&mut app, MenuAction::SinglePlayer);
    app.update();
    app
}

#[test]
fn single_player_shows_its_menu_on_the_same_panel() {
    let mut app = open_single_player(&Fixture::new());
    assert_eq!(state(&app), AppState::SinglePlayer);
    assert_eq!(
        label_order(&mut app),
        [(50, "Begin"), (51, "Resume"), (34, "Onward"), (54, "Back")]
            .map(|(id, t)| (id, t.to_string()))
    );
    let actions: Vec<MenuAction> = buttons(&mut app).into_iter().map(|(_, a)| a).collect();
    assert_eq!(
        actions,
        [MenuAction::NewGame, MenuAction::LoadGame, MenuAction::Continue, MenuAction::Cancel]
    );
    // The background stays; the main menu entries are gone.
    let world = app.world_mut();
    assert_eq!(world.query::<&Sprite>().iter(world).count(), 1);
    assert!(!label_order(&mut app).iter().any(|(id, _)| *id == 10));
    // View Intro and Last Save are not offered.
    let texts: Vec<String> = label_order(&mut app).into_iter().map(|(_, t)| t).collect();
    assert!(!texts.iter().any(|t| t == "Movie" || t == "Latest"), "{texts:?}");
}

#[test]
fn unbuilt_single_player_entries_are_dimmed_and_inert() {
    let mut app = open_single_player(&Fixture::new());
    for (button, action) in buttons(&mut app) {
        let world = app.world();
        if action.is_available() {
            assert!(world.get::<Button>(button).is_some(), "{action:?}");
            assert_eq!(label_color(&mut app, button), TEXT_COLOR, "{action:?}");
        } else {
            assert!(world.get::<Button>(button).is_none(), "{action:?}");
            assert!(world.get::<Hovered>(button).is_none(), "{action:?}: no hover highlight or sound");
            assert_eq!(label_color(&mut app, button), DISABLED_COLOR, "{action:?}");
        }
    }
    let available: Vec<MenuAction> =
        buttons(&mut app).into_iter().map(|(_, a)| a).filter(|a| a.is_available()).collect();
    assert_eq!(available, [MenuAction::NewGame, MenuAction::LoadGame, MenuAction::Cancel]);

    // Even if triggered, Continue does nothing.
    activate(&mut app, MenuAction::Continue);
    app.update();
    assert_eq!(state(&app), AppState::SinglePlayer);
}

#[test]
fn cancel_and_escape_return_to_the_main_menu() {
    let mut app = open_single_player(&Fixture::new());
    activate(&mut app, MenuAction::Cancel);
    app.update();
    assert_eq!(state(&app), AppState::MainMenu);
    assert_eq!(label_order(&mut app).len(), 4);

    activate(&mut app, MenuAction::SinglePlayer);
    app.update();
    press_escape(&mut app);
    assert_eq!(state(&app), AppState::MainMenu);
    assert_eq!(label_order(&mut app).len(), 4);
}

#[test]
fn new_game_goes_through_the_loading_screen_into_the_world() {
    let mut app = open_single_player(&Fixture::new());
    activate(&mut app, MenuAction::NewGame);
    assert_eq!(state(&app), AppState::NewGame);
    press_new_game(&mut app, NewGameAction::Next);
    assert_eq!(state(&app), AppState::Loading);
    assert!(buttons(&mut app).is_empty());
    let world = app.world_mut();
    assert_eq!(world.query::<&LoadingScreen>().iter(world).count(), 1);
    let picture = world.query::<&LoadingPicture>().single(world).unwrap().0;
    assert!(picture < paths::backgrounds::LOADING.len());
    assert_eq!(world.query::<&LoadingIndicator>().iter(world).count(), 1);

    // Escape cannot leave the loading screen.
    press_escape(&mut app);
    assert_eq!(state(&app), AppState::Loading);

    // Nothing to load: it stays up for the minimum time, then the world placeholder follows.
    update_until(&mut app, |app| state(app) == AppState::InGame);
    let world = app.world_mut();
    assert_eq!(world.query::<&LoadingScreen>().iter(world).count(), 0, "loading screen removed");
    assert!(world.get_resource::<LoadRequest>().is_none());
    assert_eq!(texts_under::<PlaceholderScreen>(&mut app), ["The world goes here", RETURN_HINT]);

    press_escape(&mut app);
    assert_eq!(state(&app), AppState::MainMenu);
    assert_eq!(label_order(&mut app).len(), 4);
}

/// Opens the loading screen with `request`, from a ready main menu.
fn start_loading(fx: &Fixture, request: LoadRequest) -> App {
    let mut app = ready_menu(fx);
    app.insert_resource(request);
    app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Loading);
    app.update();
    app.update();
    app
}

#[test]
fn loading_waits_for_its_assets_and_the_minimum_time() {
    let fx = Fixture::with_archive(
        DatBuilder::new()
            .raw(paths::backgrounds::MAIN_MENU, &menu_art())
            .raw(paths::text::MAIN_MENU, MENU_MES)
            .raw("art/big.art", &ArtBuilder::single(TestFrame::raw(2, 2, &[1, 1, 1, 1])).build()),
    );
    let mut app = ready_menu(&fx);
    let handle: Handle<Image> = app.world().resource::<AssetServer>().load("art/big.art");
    app.insert_resource(LoadRequest::new(AppState::Credits).with(handle.clone()));
    app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Loading);
    app.update();
    assert_eq!(state(&app), AppState::Loading);
    update_until(&mut app, |app| app.world().resource::<LoadingProgress>().done == 1);
    let progress = *app.world().resource::<LoadingProgress>();
    assert_eq!((progress.total, progress.fraction()), (1, 1.0));
    if progress.elapsed < MIN_LOADING_SECS - 0.2 {
        assert_eq!(state(&app), AppState::Loading, "loaded, but not shown long enough yet");
    }
    update_until(&mut app, |app| state(app) == AppState::Credits);
    assert!(app.world().get_resource::<LoadingProgress>().is_none());
}

#[test]
fn a_failed_asset_does_not_block_loading() {
    let mut app = ready_menu(&Fixture::new());
    let missing: Handle<Image> = app.world().resource::<AssetServer>().load("art/missing.art");
    app.insert_resource(LoadRequest::new(AppState::Credits).with(missing).min_secs(0.0));
    app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Loading);
    app.update();
    update_until(&mut app, |app| state(app) == AppState::Credits);
}

#[test]
fn loading_without_a_request_returns_to_the_menu() {
    let mut app = ready_menu(&Fixture::new());
    app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Loading);
    app.update();
    assert_eq!(state(&app), AppState::Loading);
    update_until(&mut app, |app| state(app) == AppState::MainMenu);
}

#[test]
fn a_long_minimum_keeps_the_loading_screen_up() {
    let mut app = start_loading(&Fixture::new(), LoadRequest::new(AppState::Credits).min_secs(60.0));
    for _ in 0..100 {
        app.update();
    }
    assert_eq!(state(&app), AppState::Loading, "10 s in, 60 s minimum");
}

/// Entities playing `handle` once and despawning afterwards.
fn one_shots(app: &mut App, handle: &Handle<AudioSource>) -> usize {
    let world = app.world_mut();
    world
        .query::<(&AudioPlayer, &PlaybackSettings)>()
        .iter(world)
        .filter(|(player, settings)| player.0 == *handle && matches!(settings.mode, PlaybackMode::Despawn))
        .count()
}

fn fixture_with_sounds() -> Fixture {
    let fx = Fixture::new();
    for path in [paths::sound::MAIN_MENU_MUSIC, paths::sound::MENU_HOVER, paths::sound::MENU_CLICK] {
        fx.override_file(path, b"not decoded in tests");
    }
    fx
}

#[test]
fn menu_music_loops_at_half_volume() {
    let mut app = ready_menu(&fixture_with_sounds());
    let world = app.world_mut();
    let music: Vec<(PlaybackMode, Volume)> = world
        .query_filtered::<&PlaybackSettings, With<MenuMusic>>()
        .iter(world)
        .map(|s| (s.mode, s.volume))
        .collect();
    assert_eq!(music.len(), 1);
    assert!(matches!(music[0].0, PlaybackMode::Loop));
    assert_eq!(music[0].1, Volume::Linear(MENU_MUSIC_VOLUME));

    // The music keeps playing across menu screens.
    activate(&mut app, MenuAction::Credits);
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<(), With<MenuMusic>>().iter(world).count(), 1);
}

#[test]
fn hover_and_click_play_interface_sounds() {
    let mut app = ready_menu(&fixture_with_sounds());
    let sounds = app.world().resource::<MenuSounds>().clone();
    let (hover, click) = (sounds.hover.unwrap(), sounds.click.unwrap());

    let credits = button_for(&mut app, MenuAction::Credits);
    app.world_mut().entity_mut(credits).insert(Hovered(true));
    app.update();
    assert_eq!(one_shots(&mut app, &hover), 1);
    // Leaving does not play it again.
    app.world_mut().entity_mut(credits).insert(Hovered(false));
    app.update();
    assert_eq!(one_shots(&mut app, &hover), 1);

    assert_eq!(one_shots(&mut app, &click), 0);
    activate(&mut app, MenuAction::Credits);
    assert_eq!(one_shots(&mut app, &click), 1);
}

#[test]
fn missing_sounds_are_skipped() {
    let mut app = ready_menu(&Fixture::new());
    let sounds = app.world().resource::<MenuSounds>().clone();
    assert!(sounds.hover.is_none() && sounds.click.is_none());
    let world = app.world_mut();
    assert_eq!(world.query::<&AudioPlayer>().iter(world).count(), 0);
    // Buttons still work without sound.
    activate(&mut app, MenuAction::Credits);
    assert_eq!(state(&app), AppState::Credits);
}

fn label_fonts(app: &mut App) -> Vec<FontSource> {
    let world = app.world_mut();
    world.query_filtered::<&TextFont, With<MenuLabel>>().iter(world).map(|f| f.font.clone()).collect()
}

#[test]
fn override_font_is_used_for_labels() {
    let fx = Fixture::new();
    fx.override_file(paths::fonts::SERIF, b"font bytes are not parsed in tests");
    let mut app = ready_menu(&fx);
    let serif = app.world().resource::<ArcanumFonts>().serif.clone().expect("font handle");
    let fonts = label_fonts(&mut app);
    assert_eq!(fonts.len(), 4);
    assert!(fonts.iter().all(|f| matches!(f, FontSource::Handle(h) if *h == serif)));
}

#[test]
fn without_a_font_labels_use_the_default() {
    let mut app = ready_menu(&Fixture::new());
    assert!(app.world().resource::<ArcanumFonts>().serif.is_none());
    assert!(label_fonts(&mut app).iter().all(|f| *f == FontSource::default()));
}

// ---------------------------------------------------------------------------------------------
// Bitmap fonts

const LARGE_LINE: u32 = 6;
const SMALL_LINE: u32 = 3;

fn fixture_with_fonts() -> Fixture {
    Fixture::with_archive(
        DatBuilder::new()
            .zlib(paths::backgrounds::MAIN_MENU, &menu_art())
            .raw(paths::text::MAIN_MENU, MENU_MES)
            .raw(paths::fonts::LARGE, &font_art(LARGE_LINE))
            .raw(paths::fonts::SMALL, &font_art(SMALL_LINE)),
    )
}

/// Waits until every `BitmapText` has been rasterised.
fn run_until_text_rendered(app: &mut App) {
    update_until(app, |app| {
        let world = app.world_mut();
        let pending = world.query_filtered::<(), (With<BitmapText>, Without<BitmapTextRendered>)>().iter(world).count();
        let done = world.query_filtered::<(), With<BitmapTextRendered>>().iter(world).count();
        pending == 0 && done > 0
    });
}

fn button_label(app: &mut App, action: MenuAction) -> Entity {
    // The button is the text entity.
    button_for(app, action)
}

#[test]
fn menu_labels_use_the_game_bitmap_font() {
    let mut app = ready_menu(&fixture_with_fonts());
    run_until_text_rendered(&mut app);
    let reference = BitmapFont::parse(&font_art(LARGE_LINE)).unwrap();

    for (id, text) in label_order(&mut app) {
        let world = app.world_mut();
        let (label, bitmap) = world
            .query::<(Entity, &MenuLabel, &BitmapText)>()
            .iter(world)
            .find(|(_, l, _)| l.0 == id)
            .map(|(e, _, b)| (e, b.clone()))
            .unwrap();
        assert_eq!(bitmap.text, text);
        assert_eq!(bitmap.scale, ORIGINAL_SCALE);
        assert!(world.get::<Text>(label).is_none(), "no plain text when the bitmap font is present");

        // Sized to the rendered text plus its one-pixel shadow, at the original scale.
        assert!(bitmap.shadow);
        let rendered = reference.render(&text);
        let (w, h, expected) = compose_rgba(&rendered, true);
        assert_eq!((w, h), (rendered.width + 1, LARGE_LINE + 1));
        let node = world.get::<Node>(label).unwrap();
        assert_eq!(node.width, Val::Px(w as f32 * ORIGINAL_SCALE));
        assert_eq!(node.height, Val::Px(h as f32 * ORIGINAL_SCALE));

        // The image is the composed text and shadow, tinted with the text colour.
        let image_node = world.get::<ImageNode>(label).unwrap();
        assert_eq!(image_node.color, TEXT_COLOR);
        let image = world.resource::<Assets<Image>>().get(&image_node.image).unwrap();
        assert_eq!((image.width(), image.height()), (w, h));
        assert_eq!(image.data.as_deref(), Some(expected.as_slice()));
        // The bottom row is shadow only: black, partly transparent.
        let bottom = &expected[((h - 1) * w + 1) as usize * 4..][..4];
        assert_eq!(&bottom[..3], [0, 0, 0]);
        assert!(bottom[3] > 0);
    }
}

#[test]
fn hover_retints_bitmap_text() {
    let mut app = ready_menu(&fixture_with_fonts());
    run_until_text_rendered(&mut app);
    let credits = button_for(&mut app, MenuAction::Credits);
    let label = button_label(&mut app, MenuAction::Credits);
    let image_before = app.world().get::<ImageNode>(label).unwrap().image.clone();

    app.world_mut().entity_mut(credits).insert(Hovered(true));
    app.update();
    let image_node = app.world().get::<ImageNode>(label).unwrap();
    assert_eq!(image_node.color, HOVER_COLOR);
    assert_eq!(image_node.image, image_before, "hover only retints, it does not re-render");

    app.world_mut().entity_mut(credits).insert(Hovered(false));
    app.update();
    assert_eq!(app.world().get::<ImageNode>(label).unwrap().color, TEXT_COLOR);
}

#[test]
fn placeholder_screens_use_large_and_small_bitmap_fonts() {
    let mut app = ready_menu(&fixture_with_fonts());
    activate(&mut app, MenuAction::Credits);
    run_until_text_rendered(&mut app);
    let fonts = app.world().resource::<ArcanumFonts>().clone();
    let world = app.world_mut();
    let screen = world.query_filtered::<Entity, With<PlaceholderScreen>>().single(world).unwrap();
    let children = world.get::<Children>(screen).unwrap().to_vec();
    let used: Vec<(String, Handle<BitmapFontAsset>)> =
        children.iter().map(|c| world.get::<BitmapText>(*c).map(|b| (b.text.clone(), b.font.clone())).unwrap()).collect();
    assert_eq!(used[0], ("Thanks".to_string(), fonts.large.unwrap()));
    assert_eq!(used[1], (RETURN_HINT.to_string(), fonts.small.unwrap()));
    let hint_node = world.get::<Node>(children[1]).unwrap();
    assert_eq!(hint_node.height, Val::Px((SMALL_LINE + 1) as f32 * ORIGINAL_SCALE));
}

#[test]
fn override_ttf_takes_priority_over_bitmap_fonts() {
    let fx = fixture_with_fonts();
    fx.override_file(paths::fonts::SERIF, b"font bytes are not parsed in tests");
    let mut app = ready_menu(&fx);
    for _ in 0..5 {
        app.update();
    }
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<(), (With<MenuLabel>, With<Text>)>().iter(world).count(), 4);
    assert_eq!(world.query_filtered::<(), With<BitmapText>>().iter(world).count(), 0);
}

#[test]
fn broken_bitmap_font_falls_back_to_plain_text() {
    let fx = Fixture::with_archive(
        DatBuilder::new()
            .raw(paths::backgrounds::MAIN_MENU, &menu_art())
            .raw(paths::text::MAIN_MENU, MENU_MES)
            .raw(paths::fonts::LARGE, b"not a font")
            .raw(paths::fonts::SMALL, &font_art(SMALL_LINE)),
    );
    let mut app = ready_menu(&fx);
    update_until(&mut app, |app| {
        let world = app.world_mut();
        world.query_filtered::<(), (With<MenuLabel>, With<Text>)>().iter(world).count() == 4
    });
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<(), (With<MenuLabel>, With<BitmapText>)>().iter(world).count(), 0);
    // The words survive the fallback.
    assert_eq!(label_order(&mut app).iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["Play", "Settings", "Thanks", "Quit"]);
}

#[test]
fn changing_bitmap_text_rerenders_and_empty_text_has_no_image() {
    let mut app = ready_menu(&fixture_with_fonts());
    run_until_text_rendered(&mut app);
    let label = button_label(&mut app, MenuAction::Options);
    let reference = BitmapFont::parse(&font_art(LARGE_LINE)).unwrap();

    app.world_mut().get_mut::<BitmapText>(label).unwrap().text = "Longer text here".into();
    app.update();
    let node = app.world().get::<Node>(label).unwrap();
    assert_eq!(node.width, Val::Px((reference.measure("Longer text here") + 1) as f32 * ORIGINAL_SCALE));

    app.world_mut().get_mut::<BitmapText>(label).unwrap().text = String::new();
    app.update();
    assert!(app.world().get::<ImageNode>(label).is_none());
    assert_eq!(app.world().get::<Node>(label).unwrap().width, Val::Px(0.0));
}

#[test]
fn art_paths_still_load_as_images_alongside_the_font_loader() {
    // Both loaders claim `.art`; the requested asset type decides.
    let mut app = menu_app(fixture_with_fonts().vfs());
    let (image, state) = load::<Image>(&mut app, paths::fonts::LARGE);
    assert!(matches!(state, LoadState::Loaded));
    assert!(app.world().resource::<Assets<Image>>().get(&image).is_some());
    let (font, state) = load::<BitmapFontAsset>(&mut app, paths::backgrounds::MAIN_MENU);
    assert!(matches!(state, LoadState::Failed(_)), "the menu background is not a font");
    let _ = font;
}

// ---------------------------------------------------------------------------------------------
// Options screen

const OPTIONS_UI_MES: &[u8] = b"{100}{Campaign}{102}{Gore Off}{104}{Autofight}{105}{Weapon Swap}{106}{Run}{107}{Helpers}\
{200}{Gamma}{201}{Text Time}{202}{Floaties}{203}{Float Pace}{204}{Taunts}{300}{FX}{301}{Speech}{302}{Tunes}";

fn options_fixture() -> Fixture {
    let arrow = {
        let mut b = ArtBuilder::single(TestFrame::raw(2, 2, &[1, 1, 1, 1]));
        b.rotations[0].push(TestFrame::raw(2, 2, &[2, 2, 2, 2]));
        b.rotations[0].push(TestFrame::raw(2, 2, &[3, 3, 3, 3]));
        b.build()
    };
    let plain = ArtBuilder::single(TestFrame::raw(2, 2, &[5, 5, 5, 5])).build();
    Fixture::with_archive(
        DatBuilder::new()
            .raw(paths::backgrounds::MAIN_MENU, &menu_art())
            .raw(paths::text::MAIN_MENU, MENU_MES)
            .raw(paths::text::OPTIONS_UI, OPTIONS_UI_MES)
            .raw(paths::text::OPTIONS_OFF_ON, b"{0}{No}{1}{Yes}")
            .raw(paths::text::OPTIONS_FLOATS, b"{0}{None}{1}{Some}{2}{All}")
            .raw(paths::interface::OPTIONS_BACK, &plain)
            .raw(paths::interface::OPTION_SLIDER, &plain)
            .raw(paths::interface::OPTION_CHOICE, &plain)
            .raw(paths::interface::ARROW_LEFT, &arrow)
            .raw(paths::interface::ARROW_RIGHT, &arrow),
    )
}

fn settings_of(app: &App) -> GameSettings {
    app.world().resource::<GameSettings>().clone()
}

/// Opens Options from the main menu and waits for the screen to be built.
fn open_options(fx: &Fixture) -> App {
    let mut app = ready_menu(fx);
    activate(&mut app, MenuAction::Options);
    assert_eq!(state(&app), AppState::Options);
    update_until(&mut app, |app| {
        let world = app.world_mut();
        world.query_filtered::<(), With<OptionSlot>>().iter(world).count() > 0
    });
    app
}

fn options_button(app: &mut App, action: OptionsAction) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &OptionsAction)>().iter(world).find(|(_, a)| **a == action).map(|(e, _)| e).unwrap_or_else(|| panic!("no button for {action:?}"))
}

fn press(app: &mut App, action: OptionsAction) {
    let entity = options_button(app, action);
    app.world_mut().trigger(Activate { entity });
    app.update();
    app.update(); // let a tab rebuild or state change apply
}

/// Text under `entity` (itself or its descendants), in hierarchy order.
fn texts_in(app: &mut App, entity: Entity) -> Vec<String> {
    let world = app.world();
    let mut out = Vec::new();
    let mut stack = vec![entity];
    while let Some(e) = stack.pop() {
        if let Some(t) = world.get::<Text>(e) {
            out.push(t.0.clone());
        } else if let Some(b) = world.get::<BitmapText>(e) {
            out.push(b.text.clone());
        }
        if let Some(children) = world.get::<Children>(e) {
            stack.extend(children.iter().rev());
        }
    }
    out
}

/// Slots in on-screen order with their text: [name, value] for choices, [name] for sliders.
fn slots(app: &mut App) -> Vec<(OptionId, Vec<String>)> {
    let world = app.world_mut();
    let screen = world.query_filtered::<Entity, With<OptionsScreen>>().single(world).unwrap();
    let children = world.get::<Children>(screen).unwrap().to_vec();
    let slot_entities: Vec<(Entity, OptionId)> =
        children.into_iter().filter_map(|c| world.get::<OptionSlot>(c).map(|s| (c, s.0))).collect();
    slot_entities.into_iter().map(|(e, id)| (id, texts_in(app, e))).collect()
}

fn value_text(app: &mut App, option: OptionId) -> String {
    let world = app.world_mut();
    let entity = world.query::<(Entity, &OptionValue)>().iter(world).find(|(_, v)| v.0 == option).unwrap().0;
    texts_in(app, entity).remove(0)
}

fn title(app: &mut App) -> String {
    let world = app.world_mut();
    let screen = world.query_filtered::<Entity, With<OptionsScreen>>().single(world).unwrap();
    texts_in(app, screen).remove(0)
}

#[test]
fn options_opens_on_the_game_tab_with_game_text() {
    let mut app = open_options(&options_fixture());
    assert_eq!(title(&mut app), "Gameplay");
    assert_eq!(
        slots(&mut app),
        vec![
            (OptionId::AutoAttack, vec!["Autofight".into(), "No".into()]),
            (OptionId::CombatTaunts, vec!["Taunts".into(), "Yes".into()]),
        ]
    );
    // Tabs and Done from MainMenu.mes; the current tab is marked.
    for (action, label) in [
        (OptionsAction::Tab(OptionsTab::Game), "Gameplay"),
        (OptionsAction::Tab(OptionsTab::Video), "Picture"),
        (OptionsAction::Tab(OptionsTab::Sound), "Audio"),
        (OptionsAction::Done, "Finished"),
    ] {
        let e = options_button(&mut app, action);
        assert_eq!(texts_in(&mut app, e), [label]);
    }
    let game_tab = options_button(&mut app, OptionsAction::Tab(OptionsTab::Game));
    assert!(app.world().get::<Selected>(game_tab).is_some());
}

#[test]
fn tabs_switch_the_settings_shown() {
    let mut app = open_options(&options_fixture());
    press(&mut app, OptionsAction::Tab(OptionsTab::Video));
    assert_eq!(app.world().resource::<CurrentTab>().0, OptionsTab::Video);
    assert_eq!(title(&mut app), "Picture");
    let ids: Vec<OptionId> = slots(&mut app).into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, OptionsTab::Video.options());
    let video = options_button(&mut app, OptionsAction::Tab(OptionsTab::Video));
    let game = options_button(&mut app, OptionsAction::Tab(OptionsTab::Game));
    assert!(app.world().get::<Selected>(video).is_some());
    assert!(app.world().get::<Selected>(game).is_none());
    assert_eq!(value_text(&mut app, OptionId::Resolution), "1920 x 1080");
    assert_eq!(value_text(&mut app, OptionId::Floats), "Some");

    press(&mut app, OptionsAction::Tab(OptionsTab::Sound));
    assert_eq!(
        slots(&mut app),
        vec![
            (OptionId::Effects, vec!["FX".into()]),
            (OptionId::Voice, vec!["Speech".into()]),
            (OptionId::Music, vec!["Tunes".into()]),
        ]
    );
}

#[test]
fn removed_settings_are_not_offered() {
    let mut app = open_options(&options_fixture());
    for tab in OptionsTab::ALL {
        press(&mut app, OptionsAction::Tab(tab));
        let texts: Vec<String> = slots(&mut app).into_iter().flat_map(|(_, t)| t).collect();
        // Difficulty and combat mode were never offered; Module ("Campaign"), Violence Filter
        // ("Gore Off"), Follower Skills ("Helpers"), Auto Switch Weapons ("Weapon Swap") and
        // Always Run ("Run") were removed, and Control Followers with them.
        for removed in ["Difficulty", "Combat Mode", "Campaign", "Gore Off", "Helpers", "Weapon Swap", "Run", "Control Followers"] {
            assert!(!texts.iter().any(|t| t.contains(removed)), "{tab:?}: {texts:?}");
        }
    }
}

#[test]
fn arrows_commit_changes_immediately() {
    let mut app = open_options(&options_fixture());
    press(&mut app, OptionsAction::Step { option: OptionId::AutoAttack, delta: 1 });
    assert!(settings_of(&app).auto_attack);
    assert_eq!(value_text(&mut app, OptionId::AutoAttack), "Yes");

    press(&mut app, OptionsAction::Step { option: OptionId::CombatTaunts, delta: -1 });
    assert!(!settings_of(&app).combat_taunts);
    assert_eq!(value_text(&mut app, OptionId::CombatTaunts), "No");

    press(&mut app, OptionsAction::Tab(OptionsTab::Sound));
    let fill_width = |app: &mut App| {
        let world = app.world_mut();
        world.query::<(&SliderFill, &Node)>().iter(world).find(|(f, _)| f.0 == OptionId::Music).map(|(_, n)| n.width).unwrap()
    };
    let full = fill_width(&mut app);
    press(&mut app, OptionsAction::Step { option: OptionId::Music, delta: -1 });
    assert_eq!(settings_of(&app).music_volume, 0.9);
    let (Val::Px(full), Val::Px(less)) = (full, fill_width(&mut app)) else { panic!("pixel widths") };
    assert!((less / full - 0.9).abs() < 1e-4, "fill tracks the value: {less} of {full}");

    // Changes survive leaving the screen.
    press(&mut app, OptionsAction::Done);
    assert_eq!(state(&app), AppState::MainMenu);
    let s = settings_of(&app);
    assert!(s.auto_attack);
    assert_eq!(s.music_volume, 0.9);
}

#[test]
fn escape_leaves_options_keeping_changes() {
    let mut app = open_options(&options_fixture());
    press(&mut app, OptionsAction::Step { option: OptionId::AutoAttack, delta: 1 });
    press_escape(&mut app);
    assert_eq!(state(&app), AppState::MainMenu);
    assert!(settings_of(&app).auto_attack);
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<(), With<OptionsScreen>>().iter(world).count(), 0);
}

#[test]
fn options_reopen_on_the_game_tab() {
    let mut app = open_options(&options_fixture());
    press(&mut app, OptionsAction::Tab(OptionsTab::Sound));
    press(&mut app, OptionsAction::Done);
    activate(&mut app, MenuAction::Options);
    app.update();
    assert_eq!(title(&mut app), "Gameplay");
}

fn dialog_count(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query_filtered::<(), With<ResolutionDialog>>().iter(world).count()
}

/// Picks the next resolution with the arrow and presses Done.
fn change_resolution(app: &mut App) {
    press(app, OptionsAction::Tab(OptionsTab::Video));
    press(app, OptionsAction::Step { option: OptionId::Resolution, delta: 1 });
    press(app, OptionsAction::Done);
}

#[test]
fn resolution_arrows_only_pick_until_done() {
    let mut app = open_options(&options_fixture());
    press(&mut app, OptionsAction::Tab(OptionsTab::Video));
    press(&mut app, OptionsAction::Step { option: OptionId::Resolution, delta: 1 });
    press(&mut app, OptionsAction::Step { option: OptionId::Resolution, delta: 1 });
    assert_eq!(value_text(&mut app, OptionId::Resolution), "2560 x 1600", "the slot shows the choice");
    assert_eq!(settings_of(&app).resolution, DisplayResolution::Fhd, "but nothing is applied");
    assert_eq!(dialog_count(&mut app), 0);
    // Choices survive switching tabs.
    press(&mut app, OptionsAction::Tab(OptionsTab::Game));
    press(&mut app, OptionsAction::Tab(OptionsTab::Video));
    assert_eq!(value_text(&mut app, OptionId::Resolution), "2560 x 1600");
}

#[test]
fn there_is_no_update_button() {
    let mut app = open_options(&options_fixture());
    press(&mut app, OptionsAction::Tab(OptionsTab::Video));
    let world = app.world_mut();
    let labels: Vec<Entity> = world.query_filtered::<Entity, With<OptionsAction>>().iter(world).collect();
    for e in labels {
        assert!(!texts_in(&mut app, e).iter().any(|t| t == "Update"));
    }
}

#[test]
fn done_without_a_new_resolution_leaves_at_once() {
    let mut app = open_options(&options_fixture());
    press(&mut app, OptionsAction::Tab(OptionsTab::Video));
    // Away and back to the current resolution: nothing to confirm.
    press(&mut app, OptionsAction::Step { option: OptionId::Resolution, delta: 1 });
    press(&mut app, OptionsAction::Step { option: OptionId::Resolution, delta: -1 });
    press(&mut app, OptionsAction::Done);
    assert_eq!(dialog_count(&mut app), 0);
    assert_eq!(state(&app), AppState::MainMenu);
}

#[test]
fn done_applies_the_resolution_and_asks_before_leaving() {
    let mut app = open_options(&options_fixture());
    change_resolution(&mut app);
    assert_eq!(settings_of(&app).resolution, DisplayResolution::Qhd, "applied straight away");
    assert_eq!(dialog_count(&mut app), 1);
    assert_eq!(state(&app), AppState::Options, "still on Options until answered");
    assert_eq!(app.world().resource::<ResolutionConfirm>().previous, DisplayResolution::Fhd);
    let world = app.world_mut();
    let title = world.query_filtered::<Entity, With<ResolutionDialog>>().single(world).unwrap();
    let texts = texts_in(&mut app, title);
    assert!(texts.iter().any(|t| t == KEEP_RESOLUTION_TITLE), "{texts:?}");
    assert_eq!(KEEP_RESOLUTION_TITLE, "Confirm Resolution");

    press(&mut app, OptionsAction::KeepResolution);
    assert_eq!(settings_of(&app).resolution, DisplayResolution::Qhd);
    assert_eq!(dialog_count(&mut app), 0);
    assert!(app.world().get_resource::<ResolutionConfirm>().is_none());
    assert_eq!(state(&app), AppState::MainMenu);
}

#[test]
fn revert_restores_the_previous_resolution_and_leaves() {
    let mut app = open_options(&options_fixture());
    change_resolution(&mut app);
    press(&mut app, OptionsAction::RevertResolution);
    assert_eq!(settings_of(&app).resolution, DisplayResolution::Fhd);
    assert_eq!(dialog_count(&mut app), 0);
    assert_eq!(state(&app), AppState::MainMenu);
    // The discarded pick is gone next time.
    activate(&mut app, MenuAction::Options);
    app.update();
    press(&mut app, OptionsAction::Tab(OptionsTab::Video));
    assert_eq!(value_text(&mut app, OptionId::Resolution), "1920 x 1080");
}

#[test]
fn the_screen_is_locked_while_confirming() {
    let mut app = open_options(&options_fixture());
    change_resolution(&mut app);
    press(&mut app, OptionsAction::Step { option: OptionId::Brightness, delta: 1 });
    press(&mut app, OptionsAction::Tab(OptionsTab::Sound));
    assert_eq!(settings_of(&app).brightness, 0.5);
    assert_eq!(app.world().resource::<CurrentTab>().0, OptionsTab::Video);
    assert_eq!(dialog_count(&mut app), 1);
}

#[test]
fn unconfirmed_resolution_reverts_after_fifteen_seconds() {
    let mut app = open_options(&options_fixture());
    change_resolution(&mut app);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)));
    let countdown = |app: &mut App| {
        let world = app.world_mut();
        let e = world.query_filtered::<Entity, With<ResolutionCountdown>>().single(world).unwrap();
        texts_in(app, e).remove(0)
    };
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(settings_of(&app).resolution, DisplayResolution::Qhd, "still waiting after 10 s");
    assert_eq!(countdown(&mut app), "Reverting in 5 seconds");
    for _ in 0..6 {
        app.update();
    }
    assert_eq!(settings_of(&app).resolution, DisplayResolution::Fhd);
    assert_eq!(dialog_count(&mut app), 0);
    assert_eq!(state(&app), AppState::MainMenu);
}

#[test]
fn escape_confirms_like_done_and_reverts_in_the_dialog() {
    let mut app = open_options(&options_fixture());
    press(&mut app, OptionsAction::Tab(OptionsTab::Video));
    press(&mut app, OptionsAction::Step { option: OptionId::Resolution, delta: 1 });
    press_escape(&mut app);
    assert_eq!(dialog_count(&mut app), 1, "Escape asks too");
    assert_eq!(state(&app), AppState::Options);
    press_escape(&mut app);
    assert_eq!(settings_of(&app).resolution, DisplayResolution::Fhd);
    assert_eq!(dialog_count(&mut app), 0);
    assert_eq!(state(&app), AppState::MainMenu);
}

#[test]
fn only_resolution_asks_for_confirmation() {
    let mut app = open_options(&options_fixture());
    press(&mut app, OptionsAction::Tab(OptionsTab::Video));
    press(&mut app, OptionsAction::Step { option: OptionId::Fullscreen, delta: 1 });
    press(&mut app, OptionsAction::Step { option: OptionId::Brightness, delta: 1 });
    assert!(settings_of(&app).fullscreen);
    assert_eq!(settings_of(&app).brightness, 0.6);
    assert_eq!(dialog_count(&mut app), 0);
}

#[test]
fn arrow_buttons_show_hover_and_pressed_frames() {
    let mut app = open_options(&options_fixture());
    let arrow = options_button(&mut app, OptionsAction::Step { option: OptionId::AutoAttack, delta: 1 });
    let frames_handle = app.world().get::<FrameButton>(arrow).unwrap().frames.clone();
    update_until(&mut app, |app| app.world().resource::<Assets<ArtFrames>>().get(&frames_handle).is_some());
    app.update();
    let frames = app.world().resource::<Assets<ArtFrames>>().get(&frames_handle).unwrap().frames.clone();
    let shown = |app: &App| app.world().get::<ImageNode>(arrow).unwrap().image.clone();
    assert_eq!(shown(&app), frames[0]);
    app.world_mut().entity_mut(arrow).insert(Hovered(true));
    app.update();
    assert_eq!(shown(&app), frames[2]);
    app.world_mut().entity_mut(arrow).insert(Pressed);
    app.update();
    assert_eq!(shown(&app), frames[1]);
    app.world_mut().entity_mut(arrow).remove::<Pressed>().insert(Hovered(false));
    app.update();
    assert_eq!(shown(&app), frames[0]);
}

#[test]
fn arrows_use_the_button_sound_and_text_buttons_the_menu_click() {
    let fx = options_fixture();
    for path in [paths::sound::MENU_CLICK, paths::sound::BUTTON_PRESS, paths::sound::BUTTON_RELEASE] {
        fx.override_file(path, b"x");
    }
    let mut app = open_options(&fx);
    let sounds = app.world().resource::<MenuSounds>().clone();
    let (click, button) = (sounds.click.unwrap(), sounds.button_press.unwrap());
    assert!(sounds.button_release.is_some());
    let (clicks, buttons) = (one_shots(&mut app, &click), one_shots(&mut app, &button));

    press(&mut app, OptionsAction::Step { option: OptionId::AutoAttack, delta: 1 });
    assert_eq!(one_shots(&mut app, &button), buttons + 1, "arrow: the button press sound");
    assert_eq!(one_shots(&mut app, &click), clicks, "arrow: not the menu click");

    press(&mut app, OptionsAction::Tab(OptionsTab::Sound));
    assert_eq!(one_shots(&mut app, &click), clicks + 1, "tab: menu click");
    assert_eq!(one_shots(&mut app, &button), buttons + 1);
}

#[test]
fn missing_option_text_falls_back_to_english() {
    let fx = Fixture::with_archive(
        DatBuilder::new().raw(paths::backgrounds::MAIN_MENU, &menu_art()).raw(paths::text::MAIN_MENU, b"{10}{Play}{12}{Settings}"),
    );
    let mut app = open_options(&fx);
    assert_eq!(title(&mut app), "Game");
    let first = slots(&mut app).remove(0);
    assert_eq!(first, (OptionId::AutoAttack, vec!["Auto Attack".into(), "Off".into()]));
    let done = options_button(&mut app, OptionsAction::Done);
    assert_eq!(texts_in(&mut app, done), ["Done"]);
}

// ---------------------------------------------------------------------------------------------
// Saved settings

/// Opens Options with settings saved to `path`, as the game does at boot.
fn open_options_saving_to(path: &Path) -> App {
    let mut app = open_options(&options_fixture());
    let (settings, file) = config::load(Some(path.to_path_buf()), EnvOverrides::default());
    app.insert_resource(settings).insert_resource(file).add_plugins(ConfigPlugin);
    app
}

fn saved(path: &Path) -> GameSettings {
    config::parse(&std::fs::read_to_string(path).unwrap(), "test")
}

#[test]
fn leaving_options_saves_the_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cfg").join("config.toml");
    let mut app = open_options_saving_to(&path);
    press(&mut app, OptionsAction::Step { option: OptionId::AutoAttack, delta: 1 });
    press(&mut app, OptionsAction::Tab(OptionsTab::Sound));
    press(&mut app, OptionsAction::Step { option: OptionId::Music, delta: -1 });
    assert!(!path.exists(), "nothing is written while the screen is open");

    press(&mut app, OptionsAction::Done);
    app.update();
    assert_eq!(state(&app), AppState::MainMenu);
    let s = saved(&path);
    assert!(s.auto_attack);
    assert_eq!(s.music_volume, 0.9);

    // The next boot starts from the saved file.
    let (booted, _) = config::load(Some(path.clone()), EnvOverrides::default());
    assert_eq!(booted, s);
}

#[test]
fn leaving_without_changes_still_writes_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut app = open_options_saving_to(&path);
    press_escape(&mut app);
    assert_eq!(saved(&path), GameSettings::default());
}

#[test]
fn only_a_confirmed_resolution_is_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");

    let mut app = open_options_saving_to(&path);
    change_resolution(&mut app);
    assert!(!path.exists(), "not saved while the confirmation is up");
    press(&mut app, OptionsAction::RevertResolution);
    app.update();
    assert_eq!(saved(&path).resolution, DisplayResolution::Fhd);

    let mut app = open_options_saving_to(&path);
    change_resolution(&mut app);
    press(&mut app, OptionsAction::KeepResolution);
    app.update();
    assert_eq!(saved(&path).resolution, DisplayResolution::Qhd);
}

#[test]
fn menu_music_follows_the_music_setting() {
    let fx = fixture_with_sounds();
    let mut app = menu_app(fx.vfs());
    app.insert_resource(GameSettings { music_volume: 0.2, ..default() });
    run_until_menu_ready(&mut app);
    let volume = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&PlaybackSettings, With<MenuMusic>>().single(world).unwrap().volume
    };
    assert_eq!(volume(&mut app), Volume::Linear(MENU_MUSIC_VOLUME * 0.2), "starts at the saved level");

    app.world_mut().resource_mut::<GameSettings>().music_volume = 0.6;
    app.update();
    assert_eq!(volume(&mut app), Volume::Linear(MENU_MUSIC_VOLUME * 0.6), "changes apply at once");
    app.world_mut().resource_mut::<GameSettings>().music_volume = 0.0;
    app.update();
    assert_eq!(volume(&mut app), Volume::Linear(0.0));
}

#[test]
fn effects_and_voice_follow_their_settings() {
    let fx = fixture_with_sounds();
    let mut app = menu_app(fx.vfs());
    app.insert_resource(GameSettings { effects_volume: 0.4, voice_volume: 0.3, ..default() });
    run_until_menu_ready(&mut app);
    let volumes = |app: &mut App, effect: bool| -> Vec<Volume> {
        let world = app.world_mut();
        if effect {
            world.query_filtered::<&PlaybackSettings, With<SoundEffect>>().iter(world).map(|p| p.volume).collect()
        } else {
            world.query_filtered::<&PlaybackSettings, With<VoiceLine>>().iter(world).map(|p| p.volume).collect()
        }
    };

    // Interface sounds start at the Effects level.
    let credits = button_for(&mut app, MenuAction::Credits);
    app.world_mut().entity_mut(credits).insert(Hovered(true));
    app.update();
    assert_eq!(volumes(&mut app, true), [Volume::Linear(0.4)]);

    // Voice lines start at the Voice level.
    let line = app.world().resource::<MenuSounds>().click.clone().unwrap();
    let settings = app.world().resource::<GameSettings>().clone();
    app.world_mut().run_system_once(move |mut commands: Commands| {
        play_voice(&mut commands, &settings, line.clone());
    }).unwrap();
    assert_eq!(volumes(&mut app, false), [Volume::Linear(0.3)]);

    // Changes reach sounds that are already queued or playing, and new ones.
    {
        let mut s = app.world_mut().resource_mut::<GameSettings>();
        s.effects_volume = 0.9;
        s.voice_volume = 0.1;
    }
    app.update();
    assert_eq!(volumes(&mut app, true), [Volume::Linear(0.9)]);
    assert_eq!(volumes(&mut app, false), [Volume::Linear(0.1)]);
    assert_eq!(app.world().resource::<MenuSounds>().volume, Volume::Linear(0.9));
}

// ---------------------------------------------------------------------------------------------
// Load Game screen

fn open_load_game(saves: SaveGames) -> App {
    let mut app = open_single_player(&Fixture::new());
    app.insert_resource(saves);
    activate(&mut app, MenuAction::LoadGame);
    app.update();
    app.update();
    app
}

fn load_game_button(app: &mut App, action: LoadGameAction) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &LoadGameAction)>().iter(world).find(|(_, a)| **a == action).map(|(e, _)| e).unwrap()
}

fn press_load_game(app: &mut App, action: LoadGameAction) {
    let entity = load_game_button(app, action);
    app.world_mut().trigger(Activate { entity });
    app.update();
    app.update();
}

fn save_rows(app: &mut App) -> Vec<String> {
    let world = app.world_mut();
    let mut rows: Vec<(usize, Entity)> = world.query::<(Entity, &SaveRow)>().iter(world).map(|(e, r)| (r.0, e)).collect();
    rows.sort();
    rows.into_iter().map(|(_, e)| texts_in(app, e).remove(0)).collect()
}

#[test]
fn load_game_opens_from_the_single_player_menu_and_goes_back() {
    let mut app = open_load_game(SaveGames::default());
    assert_eq!(state(&app), AppState::LoadGame);
    let world = app.world_mut();
    assert_eq!(world.query::<&LoadGameScreen>().iter(world).count(), 1);
    for action in [LoadGameAction::Back, LoadGameAction::Load, LoadGameAction::Delete, LoadGameAction::ToggleInfo] {
        load_game_button(&mut app, action);
    }

    press_load_game(&mut app, LoadGameAction::Back);
    assert_eq!(state(&app), AppState::SinglePlayer);
    activate(&mut app, MenuAction::LoadGame);
    app.update();
    press_escape(&mut app);
    assert_eq!(state(&app), AppState::SinglePlayer, "Escape also goes back one level");
}

#[test]
fn without_saves_the_list_says_so_and_load_does_nothing() {
    let mut app = open_load_game(SaveGames::default());
    let world = app.world_mut();
    let screen = world.query_filtered::<Entity, With<LoadGameScreen>>().single(world).unwrap();
    assert!(texts_in(&mut app, screen).iter().any(|t| t == NO_SAVES));
    assert!(save_rows(&mut app).is_empty());
    press_load_game(&mut app, LoadGameAction::Load);
    press_load_game(&mut app, LoadGameAction::Delete);
    assert_eq!(state(&app), AppState::LoadGame);
}

#[test]
fn saves_are_listed_and_can_be_selected_inspected_and_deleted() {
    let mut app = open_load_game(sample_saves());
    assert_eq!(save_rows(&mut app), ["Before the bridge", "Shrouded Hills", "Crash site"]);
    assert_eq!(app.world().resource::<LoadGameState>().selected, Some(0), "newest selected");

    press_load_game(&mut app, LoadGameAction::Select(1));
    assert_eq!(app.world().resource::<LoadGameState>().selected, Some(1));
    let selected_row = {
        let world = app.world_mut();
        world.query::<(Entity, &SaveRow)>().iter(world).find(|(_, r)| r.0 == 1).unwrap().0
    };
    assert_eq!(app.world().get::<TextColor>(selected_row).unwrap().0, HOVER_COLOR);

    // Info display: off until toggled.
    let info_count = |app: &mut App| {
        let world = app.world_mut();
        world.query::<&SaveInfo>().iter(world).count()
    };
    assert_eq!(info_count(&mut app), 0);
    press_load_game(&mut app, LoadGameAction::ToggleInfo);
    let info = {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<SaveInfo>>().single(world).unwrap()
    };
    assert_eq!(texts_in(&mut app, info), ["Shrouded Hills", "Level 3", "Shrouded Hills", "12 Nov 2026 21:05"]);
    press_load_game(&mut app, LoadGameAction::ToggleInfo);
    assert_eq!(info_count(&mut app), 0);

    press_load_game(&mut app, LoadGameAction::Delete);
    assert_eq!(save_rows(&mut app), ["Before the bridge", "Crash site"]);
    assert_eq!(app.world().resource::<LoadGameState>().selected, Some(1), "the next save is selected");
}

#[test]
fn load_goes_through_the_loading_screen() {
    let mut app = open_load_game(sample_saves());
    press_load_game(&mut app, LoadGameAction::Load);
    assert_eq!(state(&app), AppState::Loading);
    update_until(&mut app, |app| state(app) == AppState::InGame);
}

#[test]
fn hovering_a_button_shows_its_help_in_the_bottom_bar() {
    let mut app = open_load_game(SaveGames::default());
    let help = |app: &mut App| {
        let world = app.world_mut();
        let e = world.query_filtered::<Entity, With<HoverHelpText>>().single(world).unwrap();
        texts_in(app, e).remove(0)
    };
    assert_eq!(help(&mut app), "");
    let delete = load_game_button(&mut app, LoadGameAction::Delete);
    app.world_mut().entity_mut(delete).insert(Hovered(true));
    app.update();
    assert_eq!(help(&mut app), "Delete Save Game");
    app.world_mut().entity_mut(delete).insert(Hovered(false));
    app.update();
    assert_eq!(help(&mut app), "");
}

// ---------------------------------------------------------------------------------------------
// New Game screen

fn press_new_game(app: &mut App, action: NewGameAction) {
    let entity = {
        let world = app.world_mut();
        world.query::<(Entity, &NewGameAction)>().iter(world).find(|(_, a)| **a == action).map(|(e, _)| e).unwrap()
    };
    app.world_mut().trigger(Activate { entity });
    app.update();
    app.update();
}

#[test]
fn new_game_shows_its_screen_and_goes_back() {
    let mut app = open_single_player(&Fixture::new());
    activate(&mut app, MenuAction::NewGame);
    app.update();
    let world = app.world_mut();
    assert_eq!(world.query::<&NewGameScreen>().iter(world).count(), 1);
    assert_eq!(world.query::<&NewGameAction>().iter(world).count(), 2, "Back and Next");
    // The shared frame's hover help works here too.
    let next = {
        let world = app.world_mut();
        world.query::<(Entity, &NewGameAction)>().iter(world).find(|(_, a)| **a == NewGameAction::Next).unwrap().0
    };
    app.world_mut().entity_mut(next).insert(Hovered(true));
    app.update();
    let help = {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<HoverHelpText>>().single(world).unwrap()
    };
    assert_eq!(texts_in(&mut app, help), ["Next Screen"]);

    press_new_game(&mut app, NewGameAction::Back);
    assert_eq!(state(&app), AppState::SinglePlayer);
    let world = app.world_mut();
    assert_eq!(world.query::<&NewGameScreen>().iter(world).count(), 0);

    activate(&mut app, MenuAction::NewGame);
    app.update();
    press_escape(&mut app);
    assert_eq!(state(&app), AppState::SinglePlayer, "Escape goes back too");
}

// ---------------------------------------------------------------------------------------------
// Brightness

#[test]
fn brightness_follows_the_setting_on_the_camera() {
    use arcanum_client::brightness::{Brightness, brightness_exponent};
    let fx = Fixture::new();
    let mut app = menu_app(fx.vfs());
    app.add_plugins(arcanum_client::BrightnessPlugin);
    app.insert_resource(GameSettings { brightness: 0.2, ..default() });
    run_until_menu_ready(&mut app);
    app.update();
    let exponent = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&Brightness, With<Camera2d>>().single(world).unwrap().exponent
    };
    assert_eq!(exponent(&mut app), brightness_exponent(0.2));
    assert!(exponent(&mut app) > 1.0, "below the middle darkens");

    app.world_mut().resource_mut::<GameSettings>().brightness = 0.9;
    app.update();
    assert_eq!(exponent(&mut app), brightness_exponent(0.9));
    assert!(exponent(&mut app) < 1.0, "above the middle lightens");
}
