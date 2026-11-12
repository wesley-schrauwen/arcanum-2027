//! Serves Bevy's default asset source from the layered [`Vfs`].

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use arcanum_vfs::Vfs;
use bevy::asset::io::{
    AssetReader, AssetReaderError, AssetSourceBuilder, AssetSourceId, PathStream, Reader, VecReader,
};
use bevy::prelude::*;
use bevy::tasks::futures_lite::stream;

/// The shared VFS. Reads take the read lock; mounting new layers at runtime takes the write lock.
pub type SharedVfs = Arc<RwLock<Vfs>>;

/// [`AssetReader`] that resolves every asset path through the VFS.
#[derive(Clone)]
pub struct VfsAssetReader {
    vfs: SharedVfs,
}

impl VfsAssetReader {
    pub fn new(vfs: SharedVfs) -> Self {
        Self { vfs }
    }
}

fn vfs_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

impl AssetReader for VfsAssetReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let bytes = self
            .vfs
            .read()
            .map_err(|_| std::io::Error::other("VFS lock poisoned"))?
            .read(&vfs_path(path))
            .map_err(|e| match e.kind() {
                ErrorKind::NotFound => AssetReaderError::NotFound(path.to_path_buf()),
                _ => AssetReaderError::Io(Arc::new(e)),
            })?;
        Ok(VecReader::new(bytes))
    }

    /// The game's data has no `.meta` files; Bevy falls back to default loader settings.
    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        Err::<VecReader, _>(AssetReaderError::NotFound(path.to_path_buf()))
    }

    async fn read_directory<'a>(&'a self, path: &'a Path) -> Result<Box<PathStream>, AssetReaderError> {
        let children = self
            .vfs
            .read()
            .map_err(|_| std::io::Error::other("VFS lock poisoned"))?
            .list_dir(&vfs_path(path))
            .ok_or_else(|| AssetReaderError::NotFound(path.to_path_buf()))?;
        let paths: Vec<PathBuf> = children.into_iter().map(PathBuf::from).collect();
        Ok(Box::new(stream::iter(paths)))
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        Ok(self
            .vfs
            .read()
            .map_err(|_| std::io::Error::other("VFS lock poisoned"))?
            .is_dir(&vfs_path(path)))
    }
}

/// Replaces Bevy's default asset source with the VFS.
///
/// Add this **before** `DefaultPlugins` (or `AssetPlugin`): asset sources are
/// built when `AssetPlugin` is added.
pub struct VfsAssetPlugin {
    pub vfs: SharedVfs,
}

impl VfsAssetPlugin {
    pub fn new(vfs: SharedVfs) -> Self {
        Self { vfs }
    }
}

impl Plugin for VfsAssetPlugin {
    fn build(&self, app: &mut App) {
        let reader = VfsAssetReader::new(self.vfs.clone());
        app.insert_resource(VfsResource(self.vfs.clone()));
        app.register_asset_source(
            AssetSourceId::Default,
            AssetSourceBuilder::new(move || Box::new(reader.clone())),
        );
    }
}

/// Handle to the VFS for game systems that need direct access.
#[derive(Resource, Clone)]
pub struct VfsResource(pub SharedVfs);
