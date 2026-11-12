//! Arcanum client building blocks: Bevy asset integration for the game's VFS.

pub mod art_loader;
pub mod vfs_asset;

pub use art_loader::ArtAssetLoader;
pub use vfs_asset::{SharedVfs, VfsAssetPlugin, VfsAssetReader};
