//! Display settings: window resolution, fullscreen, and keeping the UI scaled to match.
//!
//! All UI is authored against a 1080-pixel-tall baseline. [`UiScale`] is derived from the
//! window's logical height, so layouts, text and hit areas stay proportional at any resolution.
//! Settings are not persisted yet; `ARCANUM_RES=WxH` and `ARCANUM_FULLSCREEN=1` override the defaults.

use std::str::FromStr;

use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode, WindowResolution};

/// Height, in logical units, that all UI and camera layout is authored against.
pub const BASELINE_HEIGHT: f32 = 1080.0;

const MIN_DIMENSION: u32 = 640;
const MAX_DIMENSION: u32 = 16384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayResolution {
    /// 1600x900 (16:9)
    HdPlus,
    /// 1920x1080 (16:9)
    Fhd,
    /// 2560x1440 (16:9)
    Qhd,
    /// 2560x1600 (16:10)
    Wqxga,
    /// 3840x2160 (16:9)
    Uhd4k,
    /// 3440x1440 (21:9)
    UltrawideQhd,
    Custom(u32, u32),
}

impl DisplayResolution {
    pub fn dimensions(&self) -> (u32, u32) {
        match *self {
            Self::HdPlus => (1600, 900),
            Self::Fhd => (1920, 1080),
            Self::Qhd => (2560, 1440),
            Self::Wqxga => (2560, 1600),
            Self::Uhd4k => (3840, 2160),
            Self::UltrawideQhd => (3440, 1440),
            Self::Custom(w, h) => (w, h),
        }
    }

    /// The named preset for these dimensions, or `Custom`.
    pub fn from_dimensions(width: u32, height: u32) -> Self {
        [Self::HdPlus, Self::Fhd, Self::Qhd, Self::Wqxga, Self::Uhd4k, Self::UltrawideQhd]
            .into_iter()
            .find(|r| r.dimensions() == (width, height))
            .unwrap_or(Self::Custom(width, height))
    }
}

/// Parses `WIDTHxHEIGHT` (e.g. `2560x1440`).
impl FromStr for DisplayResolution {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let (w, h) = s
            .trim()
            .split_once(['x', 'X'])
            .ok_or_else(|| format!("expected WIDTHxHEIGHT, got {s:?}"))?;
        let parse = |v: &str| v.trim().parse::<u32>().map_err(|_| format!("invalid number {v:?} in {s:?}"));
        let (w, h) = (parse(w)?, parse(h)?);
        if !(MIN_DIMENSION..=MAX_DIMENSION).contains(&w) || !(MIN_DIMENSION..=MAX_DIMENSION).contains(&h) {
            return Err(format!("{w}x{h} is outside {MIN_DIMENSION}..={MAX_DIMENSION} pixels"));
        }
        Ok(Self::from_dimensions(w, h))
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct GameSettings {
    pub resolution: DisplayResolution,
    pub fullscreen: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self { resolution: DisplayResolution::Fhd, fullscreen: false }
    }
}

impl GameSettings {
    /// Defaults, overridden by `ARCANUM_RES` and `ARCANUM_FULLSCREEN` when set.
    pub fn from_env() -> Self {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// Like [`Self::from_env`], reading variables through `var` (for tests).
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Self {
        let mut settings = Self::default();
        if let Some(value) = var("ARCANUM_RES") {
            match value.parse() {
                Ok(res) => settings.resolution = res,
                Err(e) => eprintln!("ignoring ARCANUM_RES: {e}"),
            }
        }
        if let Some(value) = var("ARCANUM_FULLSCREEN") {
            settings.fullscreen = matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes");
        }
        settings
    }

    /// The window resolution in physical pixels.
    pub fn window_resolution(&self) -> WindowResolution {
        let (w, h) = self.resolution.dimensions();
        WindowResolution::new(w, h)
    }

    pub fn window_mode(&self) -> WindowMode {
        if self.fullscreen {
            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
        } else {
            WindowMode::Windowed
        }
    }
}

/// UI scale for a window with the given logical height.
pub fn ui_scale_for(logical_height: f32) -> f32 {
    logical_height / BASELINE_HEIGHT
}

pub struct DisplayPlugin;

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (sync_window.run_if(resource_changed::<GameSettings>), sync_ui_scale).chain());
    }
}

/// Applies runtime changes to [`GameSettings`] to the window.
fn sync_window(settings: Res<GameSettings>, mut window: Single<&mut Window>) {
    let (w, h) = settings.resolution.dimensions();
    if window.resolution.physical_size() != UVec2::new(w, h) {
        let scale = window.resolution.scale_factor();
        window.resolution.set(w as f32 / scale, h as f32 / scale);
    }
    if window.mode != settings.window_mode() {
        window.mode = settings.window_mode();
    }
}

/// Keeps [`UiScale`] at `window height / 1080`. Runs every frame so it also follows the user
/// resizing the window or the OS changing the scale factor; it only writes on change.
fn sync_ui_scale(window: Single<&Window>, mut ui_scale: ResMut<UiScale>) {
    let scale = ui_scale_for(window.resolution.height());
    if (ui_scale.0 - scale).abs() > 1e-4 {
        ui_scale.0 = scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_have_expected_dimensions() {
        assert_eq!(DisplayResolution::HdPlus.dimensions(), (1600, 900));
        assert_eq!(DisplayResolution::Fhd.dimensions(), (1920, 1080));
        assert_eq!(DisplayResolution::Qhd.dimensions(), (2560, 1440));
        assert_eq!(DisplayResolution::Wqxga.dimensions(), (2560, 1600));
        assert_eq!(DisplayResolution::Uhd4k.dimensions(), (3840, 2160));
        assert_eq!(DisplayResolution::UltrawideQhd.dimensions(), (3440, 1440));
        assert_eq!(DisplayResolution::Custom(1234, 777).dimensions(), (1234, 777));
    }

    #[test]
    fn parses_known_and_custom_resolutions() {
        assert_eq!("3440x1440".parse(), Ok(DisplayResolution::UltrawideQhd));
        assert_eq!(" 2560X1600 ".parse(), Ok(DisplayResolution::Wqxga));
        assert_eq!("1366x768".parse(), Ok(DisplayResolution::Custom(1366, 768)));
    }

    #[test]
    fn rejects_bad_resolutions() {
        for bad in ["", "1920", "axb", "1920x", "0x0", "100x100", "99999x1080", "-1x1080"] {
            assert!(bad.parse::<DisplayResolution>().is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn ui_scale_tracks_height_against_1080() {
        assert!((ui_scale_for(900.0) - 0.8333).abs() < 1e-3);
        assert_eq!(ui_scale_for(1080.0), 1.0);
        assert!((ui_scale_for(1440.0) - 1.3333).abs() < 1e-3);
        assert!((ui_scale_for(1600.0) - 1.4815).abs() < 1e-3);
        assert_eq!(ui_scale_for(2160.0), 2.0);
    }

    #[test]
    fn reads_overrides_from_variables() {
        let vars = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
        };
        assert_eq!(GameSettings::from_vars(vars(&[])), GameSettings::default());

        let s = GameSettings::from_vars(vars(&[("ARCANUM_RES", "3440x1440"), ("ARCANUM_FULLSCREEN", "TRUE")]));
        assert_eq!(s.resolution, DisplayResolution::UltrawideQhd);
        assert!(s.fullscreen);
        assert_eq!(s.window_mode(), WindowMode::BorderlessFullscreen(MonitorSelection::Primary));

        // Bad values fall back to the defaults.
        let s = GameSettings::from_vars(vars(&[("ARCANUM_RES", "huge"), ("ARCANUM_FULLSCREEN", "nope")]));
        assert_eq!(s, GameSettings::default());
    }

    #[test]
    fn window_resolution_is_physical_pixels() {
        let s = GameSettings { resolution: DisplayResolution::Qhd, fullscreen: false };
        let r = s.window_resolution();
        assert_eq!((r.physical_width(), r.physical_height()), (2560, 1440));
    }

    #[test]
    fn default_is_windowed_1080p() {
        let s = GameSettings::default();
        assert_eq!(s.resolution, DisplayResolution::Fhd);
        assert!(!s.fullscreen);
        assert_eq!(s.window_mode(), WindowMode::Windowed);
    }
}
