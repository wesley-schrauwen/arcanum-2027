//! Saving and loading [`GameSettings`] as `config.toml`.
//!
//! The file lives in the user's config directory, separate from the game data, so it survives
//! game updates and read-only installs:
//! - macOS: `~/Library/Application Support/open-arcanum/config.toml`
//! - Linux: `$XDG_CONFIG_HOME/open-arcanum/config.toml` (usually `~/.config/...`)
//! - Windows: `%APPDATA%\open-arcanum\config.toml`
//!
//! `ARCANUM_CONFIG=<file>` points somewhere else instead (for tests and isolated runs).
//!
//! At boot, settings are resolved in increasing priority: defaults, then the file, then the
//! environment overrides (`ARCANUM_RES`, `ARCANUM_FULLSCREEN`). This happens before the window
//! is created, so it opens at the saved size.
//!
//! Loading is forgiving: a missing file means defaults; a setting that is missing, unknown or
//! invalid is skipped (with a warning) and keeps its default, without losing the others; values
//! out of range are clamped. The file is written when the player leaves the Options screen,
//! atomically (a temporary file, then a rename), so a crash never leaves it half-written.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::main_menu::AppState;
use crate::settings::{EnvOverrides, GameSettings};

/// Environment variable naming the config file to use instead of the default location.
pub const CONFIG_VAR: &str = "ARCANUM_CONFIG";
/// Folder in the user's config directory.
pub const CONFIG_DIR_NAME: &str = "open-arcanum";
pub const CONFIG_FILE_NAME: &str = "config.toml";

const HEADER: &str = "# Open Arcanum settings, written by the game when you leave the Options screen.\n\
# Sliders run from 0.0 to 1.0. Resolution is WIDTHxHEIGHT.\n\n";

/// Where settings are saved, and what the file held, so environment overrides are not saved.
#[derive(Resource, Debug, Clone)]
pub struct ConfigFile {
    /// `None` when no config directory could be found; nothing is saved then.
    pub path: Option<PathBuf>,
    /// The settings as last read from or written to the file.
    pub saved: GameSettings,
    /// Settings forced by the environment for this run.
    pub overrides: EnvOverrides,
    /// Problems found while loading. Loading happens before logging is set up, so these are
    /// logged once the app starts.
    pub load_warnings: Vec<String>,
}

impl ConfigFile {
    /// The settings to write: the current ones, except that a setting still at its
    /// environment-forced value keeps what the file had. Running once with `ARCANUM_RES` does not
    /// change the saved resolution, unless the player picks a new one in Options.
    pub fn to_save(&self, current: &GameSettings) -> GameSettings {
        let mut out = current.clone();
        if self.overrides.resolution == Some(current.resolution) {
            out.resolution = self.saved.resolution;
        }
        if self.overrides.fullscreen == Some(current.fullscreen) {
            out.fullscreen = self.saved.fullscreen;
        }
        out
    }

    /// Writes `current` (minus environment overrides) to the file. Errors are logged, not fatal.
    pub fn save(&mut self, current: &GameSettings) {
        let Some(path) = &self.path else { return };
        let mut settings = self.to_save(current);
        settings.sanitize();
        match write_atomically(path, &to_toml(&settings)) {
            Ok(()) => self.saved = settings,
            Err(e) => error!("cannot save settings to {}: {e}", path.display()),
        }
    }
}

/// The default config file location, or `None` if the OS has no config directory.
pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
}

/// The config file for this run: `ARCANUM_CONFIG` if set, else [`default_path`].
pub fn path_from_env() -> Option<PathBuf> {
    match std::env::var_os(CONFIG_VAR) {
        Some(path) if !path.is_empty() => Some(PathBuf::from(path)),
        _ => default_path(),
    }
}

/// Resolves the settings for this run and the [`ConfigFile`] to save them through.
pub fn load(path: Option<PathBuf>, overrides: EnvOverrides) -> (GameSettings, ConfigFile) {
    let mut load_warnings = Vec::new();
    let saved = match &path {
        Some(path) => read_file(path, &mut load_warnings),
        None => {
            load_warnings.push("no user config folder found; settings will not be saved".into());
            GameSettings::default()
        }
    };
    let mut settings = saved.clone();
    overrides.apply(&mut settings);
    (settings, ConfigFile { path, saved, overrides, load_warnings })
}

/// Settings from `path`; defaults if it is missing or unreadable.
fn read_file(path: &Path, warnings: &mut Vec<String>) -> GameSettings {
    match fs::read_to_string(path) {
        Ok(text) => parse_with_warnings(&text, &path.display().to_string(), warnings),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => GameSettings::default(),
        Err(e) => {
            warnings.push(format!("cannot read {}: {e}; using default settings", path.display()));
            GameSettings::default()
        }
    }
}

/// Parses config text leniently: each setting is taken on its own, so one bad value only resets
/// that setting. Problems are logged; `source` names the file.
pub fn parse(text: &str, source: &str) -> GameSettings {
    let mut warnings = Vec::new();
    let settings = parse_with_warnings(text, source, &mut warnings);
    for w in warnings {
        warn!("{w}");
    }
    settings
}

/// Like [`parse`], collecting the problems in `warnings` instead of logging them.
pub fn parse_with_warnings(text: &str, source: &str, warnings: &mut Vec<String>) -> GameSettings {
    let file: toml::Table = match toml::from_str(text) {
        Ok(table) => table,
        Err(e) => {
            warnings.push(format!("{source} is not valid TOML ({}); using default settings", e.message()));
            return GameSettings::default();
        }
    };
    let mut accepted = toml::Table::try_from(GameSettings::default()).expect("settings serialize to a table");
    for (key, value) in file {
        if !accepted.contains_key(&key) {
            warnings.push(format!("{source}: ignoring unknown setting {key:?}"));
            continue;
        }
        let mut candidate = accepted.clone();
        candidate.insert(key.clone(), value);
        match candidate.clone().try_into::<GameSettings>() {
            Ok(_) => accepted = candidate,
            Err(e) => warnings.push(format!("{source}: ignoring {key}: {}", e.message())),
        }
    }
    let mut settings: GameSettings = accepted.try_into().expect("every accepted setting was checked");
    for name in settings.sanitize() {
        warnings.push(format!("{source}: {name} was out of range and has been corrected"));
    }
    settings
}

/// The file's contents for `settings`.
pub fn to_toml(settings: &GameSettings) -> String {
    format!("{HEADER}{}", toml::to_string(settings).expect("settings serialize to TOML"))
}

/// Writes `contents` to a temporary file beside `path`, flushes it to disk, then renames it over
/// `path`. Creates the parent folders if needed.
pub fn write_atomically(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Saves the settings whenever the player leaves the Options screen. Needs a [`ConfigFile`]
/// resource; without one (as in most tests) nothing is saved.
pub struct ConfigPlugin;

impl Plugin for ConfigPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, log_load_warnings).add_systems(OnExit(AppState::Options), save_settings);
    }
}

fn log_load_warnings(config: Option<ResMut<ConfigFile>>) {
    if let Some(mut config) = config {
        for w in config.load_warnings.drain(..) {
            warn!("{w}");
        }
    }
}

fn save_settings(config: Option<ResMut<ConfigFile>>, settings: Res<GameSettings>) {
    if let Some(mut config) = config {
        config.save(&settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{DisplayResolution, Floats};

    #[test]
    fn round_trips_every_setting() {
        let s = GameSettings {
            resolution: DisplayResolution::Custom(1366, 768),
            fullscreen: true,
            auto_attack: true,
            combat_taunts: false,
            brightness: 0.1,
            text_duration: 0.2,
            floats: Floats::Verbose,
            float_speed: 0.3,
            effects_volume: 0.4,
            voice_volume: 0.6,
            music_volume: 0.7,
        };
        let text = to_toml(&s);
        assert!(text.starts_with("# Open Arcanum settings"));
        assert!(text.contains("resolution = \"1366x768\""), "{text}");
        assert!(text.contains("floats = \"verbose\""), "{text}");
        assert_eq!(parse(&text, "test"), s);
    }

    #[test]
    fn missing_settings_keep_their_defaults() {
        let s = parse("music_volume = 0.2\n", "test");
        assert_eq!(s, GameSettings { music_volume: 0.2, ..default() });
        assert_eq!(parse("", "test"), GameSettings::default());
    }

    #[test]
    fn a_bad_setting_only_resets_itself() {
        let text = "resolution = \"1024x768\"\nfloats = \"loud\"\nauto_attack = \"maybe\"\n\
                    music_volume = 0.3\nfullscreen = true\nfuture_setting = 1\n";
        let s = parse(text, "test");
        assert_eq!(s, GameSettings { music_volume: 0.3, fullscreen: true, ..default() });
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let s = parse("music_volume = 4.5\neffects_volume = -2\nbrightness = 0.4\n", "test");
        assert_eq!((s.music_volume, s.brightness), (1.0, 0.4));
        // An integer where a number is expected is accepted.
        assert_eq!(s.effects_volume, 0.0);
    }

    #[test]
    fn broken_files_give_defaults() {
        assert_eq!(parse("this is [not toml", "test"), GameSettings::default());
    }

    #[test]
    fn loading_problems_are_kept_for_the_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "resolution = \"huge\"\nmusic_volume = 9\nzoom = 2\n").unwrap();
        let (s, config) = load(Some(path), EnvOverrides::default());
        assert_eq!(s, GameSettings::default());
        let w = config.load_warnings.join("\n");
        assert_eq!(config.load_warnings.len(), 3, "{w}");
        for expected in ["ignoring resolution", "unknown setting \"zoom\"", "music_volume was out of range"] {
            assert!(w.contains(expected), "{expected}: {w}");
        }
        let (_, config) = load(None, EnvOverrides::default());
        assert_eq!(config.load_warnings.len(), 1);
    }

    #[test]
    fn boot_order_is_defaults_then_file_then_environment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        // No file: defaults.
        let (s, _) = load(Some(path.clone()), EnvOverrides::default());
        assert_eq!(s, GameSettings::default());
        assert!(!path.exists(), "loading never writes");

        fs::write(&path, "resolution = \"2560x1440\"\nmusic_volume = 0.4\n").unwrap();
        let (s, _) = load(Some(path.clone()), EnvOverrides::default());
        assert_eq!((s.resolution, s.music_volume), (DisplayResolution::Qhd, 0.4));

        let env = EnvOverrides { resolution: Some(DisplayResolution::Hd), fullscreen: None };
        let (s, config) = load(Some(path.clone()), env);
        assert_eq!((s.resolution, s.music_volume), (DisplayResolution::Hd, 0.4), "environment wins");
        assert_eq!(config.saved.resolution, DisplayResolution::Qhd);
    }

    #[test]
    fn saving_keeps_the_files_values_for_environment_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "resolution = \"2560x1440\"\n").unwrap();
        let env = EnvOverrides { resolution: Some(DisplayResolution::Hd), fullscreen: Some(true) };
        let (mut s, mut config) = load(Some(path.clone()), env);
        s.music_volume = 0.5;
        config.save(&s);
        let saved = parse(&fs::read_to_string(&path).unwrap(), "test");
        assert_eq!(saved.resolution, DisplayResolution::Qhd, "not the ARCANUM_RES value");
        assert!(!saved.fullscreen, "not the ARCANUM_FULLSCREEN value");
        assert_eq!(saved.music_volume, 0.5);

        // A resolution the player picked in Options is saved even with an override present.
        s.resolution = DisplayResolution::Uhd4k;
        config.save(&s);
        let saved = parse(&fs::read_to_string(&path).unwrap(), "test");
        assert_eq!(saved.resolution, DisplayResolution::Uhd4k);
    }

    #[test]
    fn writes_create_folders_and_leave_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("config.toml");
        write_atomically(&path, "first").unwrap();
        write_atomically(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        let names: Vec<_> = fs::read_dir(path.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["config.toml"]);
    }

    #[test]
    fn failed_writes_report_an_error_and_keep_the_old_file() {
        let dir = tempfile::tempdir().unwrap();
        // The target is a directory, so the final rename fails.
        let path = dir.path().join("config.toml");
        fs::create_dir(&path).unwrap();
        assert!(write_atomically(&path, "x").is_err());
        assert!(path.is_dir());
        assert!(!dir.path().join("config.toml.tmp").exists());

        let mut config = ConfigFile {
            path: Some(path),
            saved: GameSettings::default(),
            overrides: EnvOverrides::default(),
            load_warnings: Vec::new(),
        };
        config.save(&GameSettings { music_volume: 0.1, ..default() });
        assert_eq!(config.saved, GameSettings::default(), "unchanged after a failed save");
    }

    #[test]
    fn default_location_is_in_the_user_config_folder() {
        if let Some(path) = default_path() {
            assert!(path.ends_with(Path::new(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME)));
        }
    }
}
