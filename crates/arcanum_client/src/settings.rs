//! Display settings: window resolution, fullscreen, and keeping the UI scaled to match.
//!
//! All UI is authored against a 1080-pixel-tall baseline. [`UiScale`] is derived from the
//! window's logical height, so layouts, text and hit areas stay proportional at any resolution.
//! Settings are not persisted yet; `ARCANUM_RES=WxH` and `ARCANUM_FULLSCREEN=1` override the defaults.
//!
//! Supported resolutions run from 1280x720 to 7680x4320, with aspect ratios from 4:3 (the
//! original art) to 2.4:1 (ultrawide).

use std::fmt;
use std::str::FromStr;

use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode, WindowResolution};

/// Height, in logical units, that all UI and camera layout is authored against.
pub const BASELINE_HEIGHT: f32 = 1080.0;

/// Smallest supported resolution: 720p.
pub const MIN_RESOLUTION: (u32, u32) = (1280, 720);
/// Largest supported resolution: 8K UHD.
pub const MAX_RESOLUTION: (u32, u32) = (7680, 4320);
/// Narrowest supported aspect ratio, as `(width, height)`: 4:3, the shape of the original art.
pub const MIN_ASPECT: (u32, u32) = (4, 3);
/// Widest supported aspect ratio, as `(width, height)`: 2.4:1, covering 21:9 ultrawides.
pub const MAX_ASPECT: (u32, u32) = (12, 5);

/// Why a resolution is not supported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolutionError {
    /// Not in `WIDTHxHEIGHT` form.
    Malformed(String),
    TooSmall { width: u32, height: u32 },
    TooLarge { width: u32, height: u32 },
    AspectRatio { width: u32, height: u32 },
}

impl fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (min_w, min_h) = MIN_RESOLUTION;
        let (max_w, max_h) = MAX_RESOLUTION;
        match self {
            Self::Malformed(s) => write!(f, "expected WIDTHxHEIGHT, got {s:?}"),
            Self::TooSmall { width, height } => {
                write!(f, "{width}x{height} is smaller than the minimum {min_w}x{min_h}")
            }
            Self::TooLarge { width, height } => {
                write!(f, "{width}x{height} is larger than the maximum {max_w}x{max_h}")
            }
            Self::AspectRatio { width, height } => write!(
                f,
                "{width}x{height} has an unsupported aspect ratio (supported: {}:{} to {}:{})",
                MIN_ASPECT.0, MIN_ASPECT.1, MAX_ASPECT.0, MAX_ASPECT.1
            ),
        }
    }
}

impl std::error::Error for ResolutionError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayResolution {
    /// 1280x720 (16:9), the smallest supported resolution
    Hd,
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
            Self::Hd => (1280, 720),
            Self::HdPlus => (1600, 900),
            Self::Fhd => (1920, 1080),
            Self::Qhd => (2560, 1440),
            Self::Wqxga => (2560, 1600),
            Self::Uhd4k => (3840, 2160),
            Self::UltrawideQhd => (3440, 1440),
            Self::Custom(w, h) => (w, h),
        }
    }

    /// All named presets, smallest first.
    pub const PRESETS: [Self; 7] =
        [Self::Hd, Self::HdPlus, Self::Fhd, Self::Qhd, Self::Wqxga, Self::UltrawideQhd, Self::Uhd4k];

    /// The named preset for these dimensions, or `Custom`. Does not validate; see [`Self::new`].
    pub fn from_dimensions(width: u32, height: u32) -> Self {
        Self::PRESETS
            .into_iter()
            .find(|r| r.dimensions() == (width, height))
            .unwrap_or(Self::Custom(width, height))
    }

    /// A supported resolution: the matching preset, or `Custom` if it passes [`Self::check`].
    pub fn new(width: u32, height: u32) -> Result<Self, ResolutionError> {
        Self::check(width, height)?;
        Ok(Self::from_dimensions(width, height))
    }

    /// Checks the size limits and the aspect ratio.
    pub fn check(width: u32, height: u32) -> Result<(), ResolutionError> {
        let (min_w, min_h) = MIN_RESOLUTION;
        let (max_w, max_h) = MAX_RESOLUTION;
        if width < min_w || height < min_h {
            return Err(ResolutionError::TooSmall { width, height });
        }
        if width > max_w || height > max_h {
            return Err(ResolutionError::TooLarge { width, height });
        }
        // Compare width/height against the bounds without floating point.
        let (w, h) = (u64::from(width), u64::from(height));
        let too_narrow = w * u64::from(MIN_ASPECT.1) < h * u64::from(MIN_ASPECT.0);
        let too_wide = w * u64::from(MAX_ASPECT.1) > h * u64::from(MAX_ASPECT.0);
        if too_narrow || too_wide {
            return Err(ResolutionError::AspectRatio { width, height });
        }
        Ok(())
    }

    pub fn is_supported(&self) -> bool {
        let (w, h) = self.dimensions();
        Self::check(w, h).is_ok()
    }
}

/// Parses and validates `WIDTHxHEIGHT` (e.g. `2560x1440`).
impl FromStr for DisplayResolution {
    type Err = ResolutionError;

    fn from_str(s: &str) -> Result<Self, ResolutionError> {
        let malformed = || ResolutionError::Malformed(s.to_string());
        let (w, h) = s.trim().split_once(['x', 'X']).ok_or_else(malformed)?;
        let w = w.trim().parse::<u32>().map_err(|_| malformed())?;
        let h = h.trim().parse::<u32>().map_err(|_| malformed())?;
        Self::new(w, h)
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

/// Applies runtime changes to [`GameSettings`] to the window. Unsupported resolutions (an
/// unchecked `Custom`) are ignored with a warning.
fn sync_window(settings: Res<GameSettings>, mut window: Single<&mut Window>) {
    let (w, h) = settings.resolution.dimensions();
    if let Err(e) = DisplayResolution::check(w, h) {
        warn!("not applying resolution: {e}");
    } else if window.resolution.physical_size() != UVec2::new(w, h) {
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
        assert_eq!(DisplayResolution::Hd.dimensions(), (1280, 720));
        assert_eq!(DisplayResolution::HdPlus.dimensions(), (1600, 900));
        assert_eq!(DisplayResolution::Fhd.dimensions(), (1920, 1080));
        assert_eq!(DisplayResolution::Qhd.dimensions(), (2560, 1440));
        assert_eq!(DisplayResolution::Wqxga.dimensions(), (2560, 1600));
        assert_eq!(DisplayResolution::Uhd4k.dimensions(), (3840, 2160));
        assert_eq!(DisplayResolution::UltrawideQhd.dimensions(), (3440, 1440));
        assert_eq!(DisplayResolution::Custom(1234, 777).dimensions(), (1234, 777));
    }

    #[test]
    fn every_preset_is_supported_and_ordered() {
        for preset in DisplayResolution::PRESETS {
            assert!(preset.is_supported(), "{preset:?}");
            let (w, h) = preset.dimensions();
            assert_eq!(DisplayResolution::from_dimensions(w, h), preset);
        }
        let pixels: Vec<u32> = DisplayResolution::PRESETS.iter().map(|p| p.dimensions().0 * p.dimensions().1).collect();
        assert!(pixels.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(DisplayResolution::PRESETS[0].dimensions(), MIN_RESOLUTION);
    }

    #[test]
    fn parses_known_and_custom_resolutions() {
        assert_eq!("1280x720".parse(), Ok(DisplayResolution::Hd));
        assert_eq!("3440x1440".parse(), Ok(DisplayResolution::UltrawideQhd));
        assert_eq!(" 2560X1600 ".parse(), Ok(DisplayResolution::Wqxga));
        assert_eq!("1366x768".parse(), Ok(DisplayResolution::Custom(1366, 768)));
        assert_eq!("7680x4320".parse(), Ok(DisplayResolution::Custom(7680, 4320)));
    }

    #[test]
    fn rejects_malformed_resolutions() {
        for bad in ["", "1920", "axb", "1920x", "x1080", "-1x1080", "1920x1080x1", "1920 1080"] {
            assert!(matches!(bad.parse::<DisplayResolution>(), Err(ResolutionError::Malformed(_))), "{bad:?}");
        }
    }

    #[test]
    fn enforces_size_limits() {
        use ResolutionError::*;
        assert_eq!(DisplayResolution::new(1279, 720), Err(TooSmall { width: 1279, height: 720 }));
        assert_eq!(DisplayResolution::new(1280, 719), Err(TooSmall { width: 1280, height: 719 }));
        assert_eq!(DisplayResolution::new(1024, 768), Err(TooSmall { width: 1024, height: 768 }));
        assert_eq!(DisplayResolution::new(0, 0), Err(TooSmall { width: 0, height: 0 }));
        assert_eq!(DisplayResolution::new(7681, 4320), Err(TooLarge { width: 7681, height: 4320 }));
        assert_eq!(DisplayResolution::new(7680, 4321), Err(TooLarge { width: 7680, height: 4321 }));
        assert_eq!(DisplayResolution::new(u32::MAX, u32::MAX), Err(TooLarge { width: u32::MAX, height: u32::MAX }));
    }

    #[test]
    fn enforces_aspect_ratios() {
        // Boundaries are inclusive: exactly 4:3 and exactly 2.4:1 are allowed.
        assert_eq!(DisplayResolution::new(1440, 1080), Ok(DisplayResolution::Custom(1440, 1080)));
        assert_eq!(DisplayResolution::new(2592, 1080), Ok(DisplayResolution::Custom(2592, 1080)));
        // 5:4 is narrower than the art; 32:9 is wider than 2.4:1.
        assert_eq!(DisplayResolution::new(1280, 1024), Err(ResolutionError::AspectRatio { width: 1280, height: 1024 }));
        assert_eq!(DisplayResolution::new(1439, 1080), Err(ResolutionError::AspectRatio { width: 1439, height: 1080 }));
        assert_eq!(DisplayResolution::new(2593, 1080), Err(ResolutionError::AspectRatio { width: 2593, height: 1080 }));
        assert_eq!(DisplayResolution::new(5120, 1440), Err(ResolutionError::AspectRatio { width: 5120, height: 1440 }));
        // Portrait.
        assert_eq!(DisplayResolution::new(1440, 2560), Err(ResolutionError::AspectRatio { width: 1440, height: 2560 }));
        assert!(!DisplayResolution::Custom(1280, 1024).is_supported());
    }

    #[test]
    fn errors_explain_the_limits() {
        assert_eq!(
            "1024x768".parse::<DisplayResolution>().unwrap_err().to_string(),
            "1024x768 is smaller than the minimum 1280x720"
        );
        assert_eq!(
            "5120x1440".parse::<DisplayResolution>().unwrap_err().to_string(),
            "5120x1440 has an unsupported aspect ratio (supported: 4:3 to 12:5)"
        );
    }

    #[test]
    fn ui_scale_tracks_height_against_1080() {
        assert!((ui_scale_for(720.0) - 0.6667).abs() < 1e-3);
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
        // Well-formed but unsupported values are rejected too.
        for res in ["1024x768", "5120x1440", "15360x8640"] {
            let s = GameSettings::from_vars(move |name: &str| (name == "ARCANUM_RES").then(|| res.to_string()));
            assert_eq!(s.resolution, DisplayResolution::Fhd, "{res}");
        }
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
