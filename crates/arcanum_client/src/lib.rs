//! Arcanum client building blocks: Bevy asset integration for the game's VFS.

pub mod art_loader;
pub mod main_menu;
pub mod mes_loader;
pub mod vfs_asset;

use bevy::prelude::*;

pub use art_loader::ArtAssetLoader;
pub use main_menu::MainMenuPlugin;
pub use mes_loader::{MesAsset, MesAssetLoader};
pub use vfs_asset::{SharedVfs, VfsAssetPlugin, VfsAssetReader};

/// Registers the `.art` and `.mes` asset loaders. Add after `DefaultPlugins`.
pub struct ArcanumAssetsPlugin;

impl Plugin for ArcanumAssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<MesAsset>()
            .register_asset_loader(ArtAssetLoader)
            .register_asset_loader(MesAssetLoader);
    }
}
