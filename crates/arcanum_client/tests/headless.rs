//! Headless tests: no window, no GPU and no game data. Every asset comes from synthetic
//! `.dat` archives and override folders built in a temp directory.

use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use arcanum_client::main_menu::{MENU_ENTRY_IDS, MenuLabel, menu_ready};
use arcanum_client::settings::BASELINE_HEIGHT;
use arcanum_client::{
    ArcanumAssetsPlugin, DisplayPlugin, DisplayResolution, GameSettings, MainMenuPlugin, MesAsset,
    VfsAssetPlugin, VfsAssetReader,
};
use arcanum_formats::test_util::{ArtBuilder, TestFrame, ramp_palette};
use arcanum_vfs::Vfs;
use arcanum_vfs::test_util::DatBuilder;
use bevy::app::TaskPoolPlugin;
use bevy::asset::io::{AssetReader, AssetReaderError, Reader};
use bevy::asset::{AssetPlugin, LoadState};
use bevy::camera::ScalingMode;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::tasks::block_on;
use bevy::tasks::futures_lite::StreamExt;
use bevy::window::{MonitorSelection, WindowMode, WindowResolution};

// ---------------------------------------------------------------------------------------------
// Fixtures

const MENU_MES: &[u8] = b"// test menu\r\n{10}{Play}\r\n{11}{Online}\r\n{12}{Settings}\r\n{13}{Thanks}\r\n{14}{Quit}\r\n";

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
    fn new() -> Self {
        Self::with_archive(
            DatBuilder::new()
                .zlib("art\\interface\\mainmenuback.art", &menu_art())
                .raw("mes\\MainMenu.mes", MENU_MES),
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

    assert_eq!(read_all(&r, "mes/MainMenu.mes").unwrap(), MENU_MES);
    assert_eq!(read_all(&r, "art/interface/mainmenuback.art").unwrap(), menu_art());
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
    let result = block_on(async { r.read_meta(Path::new("mes/MainMenu.mes")).await.map(|_| ()) });
    assert!(matches!(result, Err(AssetReaderError::NotFound(_))));
}

#[test]
fn reader_directories() {
    let fx = Fixture::new();
    fx.override_file("art/interface/extra.art", b"");
    let r = reader(&fx);

    assert!(block_on(r.is_directory(Path::new("art/interface"))).unwrap());
    assert!(block_on(r.is_directory(Path::new(""))).unwrap());
    assert!(!block_on(r.is_directory(Path::new("mes/MainMenu.mes"))).unwrap());
    assert!(!block_on(r.is_directory(Path::new("missing"))).unwrap());

    let listed: Vec<_> = block_on(async {
        r.read_directory(Path::new("art/interface")).await.unwrap().collect::<Vec<_>>().await
    });
    let listed: Vec<String> = listed.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    assert_eq!(listed, ["art/interface/extra.art", "art/interface/mainmenuback.art"]);

    let missing = block_on(async { r.read_directory(Path::new("missing")).await.map(|_| ()) });
    assert!(matches!(missing, Err(AssetReaderError::NotFound(_))));
}

// ---------------------------------------------------------------------------------------------
// Asset loaders

#[test]
fn art_loads_as_an_rgba_image() {
    let mut app = asset_app(Fixture::new().vfs());
    let (handle, state) = load::<Image>(&mut app, "art/interface/mainmenuback.art");
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
    fx.override_file("mes/MainMenu.mes", b"{10}{Overridden}");
    let mut app = asset_app(fx.vfs());
    let (handle, _) = load::<MesAsset>(&mut app, "mes/MainMenu.mes");
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

fn menu_app(vfs: Vfs) -> App {
    let mut app = asset_app(vfs);
    app.add_plugins(MainMenuPlugin);
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
    let mut labels = world.query::<(Entity, &MenuLabel, &Text, &ChildOf)>();
    let entries: Vec<(Entity, u32, String, Entity)> =
        labels.iter(world).map(|(e, l, t, parent)| (e, l.0, t.0.clone(), parent.parent())).collect();
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
        .zip(["Play", "Online", "Settings", "Thanks", "Quit"])
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
            .raw("art/interface/mainmenuback.art", &menu_art())
            .raw("mes/MainMenu.mes", b"{10}{Play}{14}{Quit}"),
    );
    let mut app = menu_app(fx.vfs());
    run_until_menu_ready(&mut app);
    assert_eq!(label_order(&mut app), vec![(10, "Play".into()), (14, "Quit".into())]);
}

#[test]
fn main_menu_survives_missing_strings() {
    let fx = Fixture::with_archive(DatBuilder::new().raw("art/interface/mainmenuback.art", &menu_art()));
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
    let fx = Fixture::with_archive(DatBuilder::new().raw("mes/MainMenu.mes", MENU_MES));
    let mut app = menu_app(fx.vfs());
    update_until(&mut app, |app| !label_order(app).is_empty());
    assert_eq!(label_order(&mut app).len(), 5);
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
    GameSettings { resolution, fullscreen }
}

#[test]
fn ui_scale_follows_window_height() {
    for (res, scale) in [
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
