//! Where the game data comes from and which assets the client loads from it.
//!
//! Asset paths are relative to the VFS root and resolve case-insensitively through the override
//! folder and the `.dat` archives. Keeping them here, rather than scattered through systems,
//! gives one place to see (and later swap) what the client depends on.

use std::io;
use std::path::PathBuf;

use arcanum_vfs::Vfs;
use bevy::prelude::*;

/// Environment variable naming the game folder that holds the `.dat` archives.
pub const GAME_DIR_VAR: &str = "ARCANUM_DIR";
/// Environment variable naming a loose-file folder that shadows the archives.
pub const OVERRIDE_DIR_VAR: &str = "ARCANUM_OVERRIDE";
/// Game folder used when `ARCANUM_DIR` is not set, relative to the working directory.
pub const DEFAULT_GAME_DIR: &str = "assets/Arcanum";

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

    /// Message (`.mes`) files with UI and game text.
    pub mod text {
        pub const DIR: &str = "mes";
        /// Main menu strings; ids 10 to 14 are the main menu entries.
        pub const MAIN_MENU: &str = "mes/MainMenu.mes";
    }

    /// Sound effects (`.wav`).
    pub mod sound {
        pub const DIR: &str = "sound";
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
    ];
}

/// Locations of the game data.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ResourcePack {
    pub game_dir: PathBuf,
    pub override_dir: Option<PathBuf>,
}

impl ResourcePack {
    /// Reads `ARCANUM_DIR` and `ARCANUM_OVERRIDE`, defaulting to [`DEFAULT_GAME_DIR`].
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
        }
    }

    /// Mounts the override folder (if any) and every `.dat` in the game folder.
    pub fn open_vfs(&self) -> io::Result<Vfs> {
        Vfs::from_game_dir(&self.game_dir, self.override_dir.as_deref())
    }
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
        assert_eq!(pack, ResourcePack { game_dir: "fallback".into(), override_dir: None });
    }

    #[test]
    fn variables_override_defaults() {
        let pack = ResourcePack::from_vars(vars(&[(GAME_DIR_VAR, "/games/arcanum"), (OVERRIDE_DIR_VAR, "mods")]), "fallback");
        assert_eq!(pack.game_dir, PathBuf::from("/games/arcanum"));
        assert_eq!(pack.override_dir, Some(PathBuf::from("mods")));
    }

    #[test]
    fn empty_variables_are_ignored() {
        let pack = ResourcePack::from_vars(vars(&[(GAME_DIR_VAR, ""), (OVERRIDE_DIR_VAR, "  ")]), "fallback");
        assert_eq!(pack, ResourcePack { game_dir: "fallback".into(), override_dir: None });
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

    #[test]
    fn open_vfs_reports_a_missing_game_dir() {
        let pack = ResourcePack::from_vars(vars(&[]), "/definitely/not/here");
        assert!(pack.open_vfs().is_err());
    }
}
