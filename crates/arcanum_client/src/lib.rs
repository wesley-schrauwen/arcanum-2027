//! Arcanum client building blocks: Bevy asset integration for the game's VFS.

pub mod art_loader;
pub mod audio;
pub mod bitmap_text;
pub mod config;
pub mod fonts;
pub mod load_game;
pub mod loading;
pub mod main_menu;
pub mod mes_loader;
pub mod options;
pub mod resource_pack;
pub mod screens;
pub mod settings;
pub mod vfs_asset;
pub mod widgets;

use bevy::prelude::*;

pub use art_loader::{ArtAssetLoader, ArtFrames, ArtFramesLoader};
pub use audio::MenuAudioPlugin;
pub use bitmap_text::BitmapTextPlugin;
pub use config::{ConfigFile, ConfigPlugin};
pub use fonts::FontsPlugin;
pub use load_game::LoadGamePlugin;
pub use loading::LoadingPlugin;
pub use main_menu::{AppState, MainMenuPlugin};
pub use screens::ScreensPlugin;
pub use mes_loader::{MesAsset, MesAssetLoader};
pub use options::OptionsPlugin;
pub use resource_pack::ResourcePack;
pub use settings::{DisplayPlugin, DisplayResolution, GameSettings};
pub use vfs_asset::{SharedVfs, VfsAssetPlugin, VfsAssetReader};

/// Registers the `.art` (image and frame-set) and `.mes` asset loaders. Add after `DefaultPlugins`.
pub struct ArcanumAssetsPlugin;

impl Plugin for ArcanumAssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<MesAsset>()
            .init_asset::<ArtFrames>()
            .register_asset_loader(ArtAssetLoader)
            .register_asset_loader(ArtFramesLoader)
            .register_asset_loader(MesAssetLoader);
    }
}
