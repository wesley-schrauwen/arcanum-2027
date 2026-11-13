//! Where the game data comes from and which assets the client loads from it.
//!
//! Asset paths are relative to the VFS root and resolve case-insensitively through the override
//! folder and the `.dat` archives. Keeping them here, rather than scattered through systems,
//! gives one place to see (and later swap) what the client depends on.

use std::io;
use std::path::{Path, PathBuf};

use arcanum_vfs::Vfs;
use bevy::prelude::*;

/// Environment variable naming the game folder that holds the `.dat` archives.
pub const GAME_DIR_VAR: &str = "ARCANUM_DIR";
/// Environment variable naming a loose-file folder that shadows the archives.
pub const OVERRIDE_DIR_VAR: &str = "ARCANUM_OVERRIDE";
/// Environment variable naming the game module to load (a folder and archives under `modules/`).
pub const MODULE_VAR: &str = "ARCANUM_MODULE";
/// Game folder used when `ARCANUM_DIR` is not set, relative to the working directory.
pub const DEFAULT_GAME_DIR: &str = "assets/Arcanum";
/// The main campaign module.
pub const DEFAULT_MODULE: &str = "Arcanum";

/// Asset paths inside the VFS, grouped by kind. Each group has its directory root (`DIR`) and
/// the files the client knows by name. Every path here exists in the shipped archives; the
/// real-data tests check that.
pub mod paths {
    /// Full-screen backgrounds.
    pub mod backgrounds {
        /// Main menu background, 800x600.
        pub const MAIN_MENU: &str = "art/interface/mainmenuback.art";
        /// Splash screens shown at startup (`.bmp`).
        pub const SPLASH_DIR: &str = "art/splash";
    }

    /// Interface art: buttons, windows, cursors.
    pub mod interface {
        pub const DIR: &str = "art/interface";
        /// Interface art id -> file name.
        pub const INDEX: &str = "art/interface/interface.mes";
        /// Options screen background, 800x600: tab box, title plate, 2x4 grid of setting slots.
        pub const OPTIONS_BACK: &str = "art/interface/OptionsMenuBack.ART";
        /// Setting slot with a slider tube, 213x56.
        pub const OPTION_SLIDER: &str = "art/interface/OptionSlider.ART";
        /// Setting slot with a value window, 213x56.
        pub const OPTION_CHOICE: &str = "art/interface/OptionMultiChoice.ART";
        /// Left/right arrow buttons for setting slots, 29x30. Frames: normal, pressed, hover.
        pub const ARROW_LEFT: &str = "art/interface/SldrButt_L_Arrow.ART";
        pub const ARROW_RIGHT: &str = "art/interface/SldrButt_R_Arrow.ART";
    }

    /// Character portraits (`.bmp`).
    pub mod portraits {
        pub const DIR: &str = "portrait";
        /// Portrait id -> file name.
        pub const INDEX: &str = "portrait/gameport.mes";
    }

    /// Map textures: ground tiles, walls, roofs and building facades.
    pub mod textures {
        pub const TILES_DIR: &str = "art/tile";
        pub const TILE_NAMES: &str = "art/tile/tilename.mes";
        pub const WALLS_DIR: &str = "art/wall";
        pub const WALL_NAMES: &str = "art/wall/wallname.mes";
        pub const ROOFS_DIR: &str = "art/roof";
        pub const FACADES_DIR: &str = "art/facade";
        pub const FACADE_NAMES: &str = "art/facade/facadename.mes";
    }

    /// Item art, in the inventory and on the ground.
    pub mod items {
        pub const DIR: &str = "art/item";
        pub const INVENTORY_INDEX: &str = "art/item/item_inven.mes";
        pub const GROUND_INDEX: &str = "art/item/item_ground.mes";
    }

    /// Animated characters.
    pub mod critters {
        pub const DIR: &str = "art/critter";
        pub const MONSTERS_DIR: &str = "art/monster";
        pub const UNIQUE_NPCS_DIR: &str = "art/unique_npc";
    }

    /// World objects that are not items or critters.
    pub mod scenery {
        pub const DIR: &str = "art/scenery";
        pub const INDEX: &str = "art/scenery/scenery.mes";
    }

    /// Fonts. The game's own are bitmap `.art` fonts; [`SERIF`](fonts::SERIF) is an optional
    /// TrueType override that is not part of the game data (so not in `ALL_FILES`).
    pub mod fonts {
        /// Menu entries and titles.
        pub const LARGE: &str = "art/interface/Morph30Font.ART";
        /// Hints and small print.
        pub const SMALL: &str = "art/interface/morph15font.art";
        /// TrueType override for UI text. Takes priority over the bitmap fonts when present.
        pub const SERIF: &str = "fonts/arcanum_serif.ttf";
    }

    /// Message (`.mes`) files with UI and game text.
    pub mod text {
        pub const DIR: &str = "mes";
        /// Main menu strings; ids 10 to 14 are the main menu entries, 4000 to 4003 the Options
        /// screen's Game, Video, Sound and Done buttons.
        pub const MAIN_MENU: &str = "mes/MainMenu.mes";
        /// Options setting names (100s Game, 200s Video, 300s Sound).
        pub const OPTIONS_UI: &str = "mes/options_ui.mes";
        /// Option values: Off/On; floating text modes.
        pub const OPTIONS_OFF_ON: &str = "mes/OptionsOffOn.mes";
        pub const OPTIONS_FLOATS: &str = "mes/OptionsFloats.mes";
    }

    /// Sound effects (`.wav`) and music (`.mp3`, loose files in the module folder).
    pub mod sound {
        pub const DIR: &str = "sound";
        pub const MUSIC_DIR: &str = "sound/music";
        /// Main menu theme: scheme "M.Main Menu" (#2600) in `sound/schemelist.mes`, looped at 50%.
        pub const MAIN_MENU_MUSIC: &str = "sound/music/Arcanum.mp3";
        /// Menu text hover and click (ids 3026 and 3027 in `sound/snd_interface.mes`).
        pub const MENU_HOVER: &str = "sound/MorphText_HOVER.wav";
        pub const MENU_CLICK: &str = "sound/MorphText_CLICK.wav";
        /// Picture-button press and release (ids 3000 and 3001), used by the Options arrows. The
        /// original's slider arrows (`SldrButt_L_Arrow`/`SldrButt_R_Arrow`, interface art 667/668)
        /// use these, with no hover sound (Arcanum.exe around 0x57fe60).
        pub const BUTTON_PRESS: &str = "sound/Button_Medium.wav";
        pub const BUTTON_RELEASE: &str = "sound/Button_Medium_Release.wav";
    }

    /// Every file path above, for checks.
    pub const ALL_FILES: &[&str] = &[
        backgrounds::MAIN_MENU,
        interface::INDEX,
        portraits::INDEX,
        textures::TILE_NAMES,
        textures::WALL_NAMES,
        textures::FACADE_NAMES,
        items::INVENTORY_INDEX,
        items::GROUND_INDEX,
        scenery::INDEX,
        text::MAIN_MENU,
        text::OPTIONS_UI,
        text::OPTIONS_OFF_ON,
        text::OPTIONS_FLOATS,
        interface::OPTIONS_BACK,
        interface::OPTION_SLIDER,
        interface::OPTION_CHOICE,
        interface::ARROW_LEFT,
        interface::ARROW_RIGHT,
        sound::MAIN_MENU_MUSIC,
        sound::MENU_HOVER,
        sound::MENU_CLICK,
        sound::BUTTON_PRESS,
        sound::BUTTON_RELEASE,
        fonts::LARGE,
        fonts::SMALL,
    ];

    /// Every directory path above, for checks.
    pub const ALL_DIRS: &[&str] = &[
        backgrounds::SPLASH_DIR,
        interface::DIR,
        portraits::DIR,
        textures::TILES_DIR,
        textures::WALLS_DIR,
        textures::ROOFS_DIR,
        textures::FACADES_DIR,
        items::DIR,
        critters::DIR,
        critters::MONSTERS_DIR,
        critters::UNIQUE_NPCS_DIR,
        scenery::DIR,
        text::DIR,
        sound::DIR,
        sound::MUSIC_DIR,
    ];
}

/// Locations of the game data.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ResourcePack {
    pub game_dir: PathBuf,
    pub override_dir: Option<PathBuf>,
    /// Module name: `modules/<module>/`, `modules/<module>.dat` and its `.PATCH*` archives.
    pub module: String,
}

impl ResourcePack {
    /// Reads `ARCANUM_DIR`, `ARCANUM_OVERRIDE` and `ARCANUM_MODULE`, defaulting to
    /// [`DEFAULT_GAME_DIR`] and [`DEFAULT_MODULE`].
    pub fn from_env() -> Self {
        Self::from_vars(|name| std::env::var(name).ok(), DEFAULT_GAME_DIR)
    }

    /// Like [`Self::from_env`], reading variables through `var` and falling back to
    /// `default_game_dir` (for tests).
    pub fn from_vars(var: impl Fn(&str) -> Option<String>, default_game_dir: impl Into<PathBuf>) -> Self {
        let non_empty = |name| var(name).filter(|v: &String| !v.trim().is_empty()).map(PathBuf::from);
        Self {
            game_dir: non_empty(GAME_DIR_VAR).unwrap_or_else(|| default_game_dir.into()),
            override_dir: non_empty(OVERRIDE_DIR_VAR),
            module: var(MODULE_VAR)
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| DEFAULT_MODULE.to_string()),
        }
    }

    /// Mounts, highest priority first:
    /// 1. the override folder, if any;
    /// 2. the module's patch archives `modules/<module>.PATCH*`, highest number first;
    /// 3. the module folder `modules/<module>/` (loose files such as music);
    /// 4. the module archive `modules/<module>.dat`;
    /// 5. every `.dat` in the game folder.
    ///
    /// Missing module pieces are skipped; a missing game folder is an error.
    pub fn open_vfs(&self) -> io::Result<Vfs> {
        let mut vfs = Vfs::new();
        if let Some(dir) = &self.override_dir {
            vfs.mount_dir(dir);
        }
        let modules = self.game_dir.join("modules");
        for patch in module_patches(&modules, &self.module) {
            vfs.mount_archive(patch)?;
        }
        let module_dir = modules.join(&self.module);
        if module_dir.is_dir() {
            vfs.mount_dir(module_dir);
        }
        let module_dat = modules.join(format!("{}.dat", self.module));
        if module_dat.is_file() {
            vfs.mount_archive(module_dat)?;
        }
        vfs.mount_archives_in(&self.game_dir)?;
        Ok(vfs)
    }
}

/// `modules/<module>.PATCH<n>` files, highest `n` first.
fn module_patches(modules: &Path, module: &str) -> Vec<PathBuf> {
    let prefix = format!("{}.patch", module.to_ascii_lowercase());
    let mut patches: Vec<(u32, PathBuf)> = std::fs::read_dir(modules)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            let n = name.strip_prefix(&prefix)?.parse::<u32>().ok()?;
            e.path().is_file().then(|| (n, e.path()))
        })
        .collect();
    patches.sort_by(|a, b| b.0.cmp(&a.0));
    patches.into_iter().map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    }

    #[test]
    fn defaults_without_variables() {
        let pack = ResourcePack::from_vars(vars(&[]), "fallback");
        assert_eq!(pack, ResourcePack { game_dir: "fallback".into(), override_dir: None, module: DEFAULT_MODULE.into() });
    }

    #[test]
    fn variables_override_defaults() {
        let pack = ResourcePack::from_vars(vars(&[(GAME_DIR_VAR, "/games/arcanum"), (OVERRIDE_DIR_VAR, "mods")]), "fallback");
        assert_eq!(pack.game_dir, PathBuf::from("/games/arcanum"));
        assert_eq!(pack.override_dir, Some(PathBuf::from("mods")));
        let pack = ResourcePack::from_vars(vars(&[(MODULE_VAR, " Vormantown ")]), "fallback");
        assert_eq!(pack.module, "Vormantown");
    }

    #[test]
    fn empty_variables_are_ignored() {
        let pack =
            ResourcePack::from_vars(vars(&[(GAME_DIR_VAR, ""), (OVERRIDE_DIR_VAR, "  "), (MODULE_VAR, "")]), "fallback");
        assert_eq!(pack, ResourcePack { game_dir: "fallback".into(), override_dir: None, module: DEFAULT_MODULE.into() });
    }

    #[test]
    fn paths_are_relative_vfs_paths() {
        for p in paths::ALL_FILES.iter().chain(paths::ALL_DIRS) {
            assert!(!p.is_empty() && !p.starts_with('/') && !p.ends_with('/'), "{p}");
            assert!(!p.contains('\\') && !p.split('/').any(|c| c.is_empty() || c == "." || c == ".."), "{p}");
        }
    }

    #[test]
    fn files_sit_in_their_group_directory() {
        use paths::*;
        for (file, dir) in [
            (interface::INDEX, interface::DIR),
            (portraits::INDEX, portraits::DIR),
            (textures::TILE_NAMES, textures::TILES_DIR),
            (textures::WALL_NAMES, textures::WALLS_DIR),
            (textures::FACADE_NAMES, textures::FACADES_DIR),
            (items::INVENTORY_INDEX, items::DIR),
            (items::GROUND_INDEX, items::DIR),
            (scenery::INDEX, scenery::DIR),
            (text::MAIN_MENU, text::DIR),
            (backgrounds::MAIN_MENU, interface::DIR),
        ] {
            assert!(file.starts_with(&format!("{dir}/")), "{file} not in {dir}");
        }
    }

    fn write(path: &Path, data: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }

    #[test]
    fn open_vfs_layers_the_module_between_override_and_base_archives() {
        use arcanum_vfs::test_util::DatBuilder;
        let game = tempfile::tempdir().unwrap();
        let g = game.path();
        // Each layer provides its own name for "who.txt", plus one file only it has.
        DatBuilder::new().raw("who.txt", b"base").raw("base.txt", b"1").write(g.join("arcanum1.dat"));
        DatBuilder::new().raw("who.txt", b"module dat").raw("dat.txt", b"1").write(g.join("modules/Arcanum.dat"));
        write(&g.join("modules/Arcanum/who.txt"), b"module dir");
        write(&g.join("modules/Arcanum/sound/music/Theme.mp3"), b"mp3");
        DatBuilder::new().raw("who.txt", b"patch0").raw("p0.txt", b"1").write(g.join("modules/Arcanum.PATCH0"));
        DatBuilder::new().raw("who.txt", b"patch1").write(g.join("modules/Arcanum.PATCH1"));
        DatBuilder::new().raw("who.txt", b"other module").write(g.join("modules/Other.dat"));

        let pack = |override_dir: Option<PathBuf>, module: &str| ResourcePack {
            game_dir: g.to_path_buf(),
            override_dir,
            module: module.into(),
        };

        let vfs = pack(None, "Arcanum").open_vfs().unwrap();
        assert_eq!(vfs.read("who.txt").unwrap(), b"patch1");
        for f in ["base.txt", "dat.txt", "p0.txt", "sound/music/theme.mp3"] {
            assert!(vfs.exists(f), "{f}");
        }

        let over = tempfile::tempdir().unwrap();
        write(&over.path().join("who.txt"), b"override");
        let vfs = pack(Some(over.path().into()), "Arcanum").open_vfs().unwrap();
        assert_eq!(vfs.read("who.txt").unwrap(), b"override");

        // Another module sees only its own pieces plus the base archives.
        let vfs = pack(None, "Other").open_vfs().unwrap();
        assert_eq!(vfs.read("who.txt").unwrap(), b"other module");
        assert!(!vfs.exists("dat.txt"));
        assert!(vfs.exists("base.txt"));

        // A module that does not exist is not an error.
        let vfs = pack(None, "Missing").open_vfs().unwrap();
        assert_eq!(vfs.read("who.txt").unwrap(), b"base");
    }

    #[test]
    fn module_patches_sort_numerically() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["Arcanum.PATCH2", "arcanum.patch10", "Arcanum.PATCH0", "Arcanum.PATCHx", "Other.PATCH5"] {
            write(&dir.path().join(name), b"");
        }
        let names: Vec<String> = module_patches(dir.path(), "Arcanum")
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["arcanum.patch10", "Arcanum.PATCH2", "Arcanum.PATCH0"]);
    }

    #[test]
    fn open_vfs_reports_a_missing_game_dir() {
        let pack = ResourcePack::from_vars(vars(&[]), "/definitely/not/here");
        assert!(pack.open_vfs().is_err());
    }
}
