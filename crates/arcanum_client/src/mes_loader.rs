//! Bevy [`AssetLoader`] for `.mes` message files.

use arcanum_formats::mes::{MesError, MesFile};
use bevy::asset::{Asset, AssetLoader, LoadContext, io::Reader};
use bevy::reflect::TypePath;

/// A parsed `.mes` file as a Bevy asset.
#[derive(Asset, TypePath, Debug, Clone)]
pub struct MesAsset(pub MesFile);

impl MesAsset {
    pub fn get(&self, id: u32) -> Option<&str> {
        self.0.get(id)
    }
}

#[derive(Default, TypePath)]
pub struct MesAssetLoader;

#[derive(Debug)]
pub enum MesLoadError {
    Io(std::io::Error),
    Mes(MesError),
}

impl std::fmt::Display for MesLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "could not read .mes file: {e}"),
            Self::Mes(e) => write!(f, "invalid .mes file: {e}"),
        }
    }
}

impl std::error::Error for MesLoadError {}

impl AssetLoader for MesAssetLoader {
    type Asset = MesAsset;
    type Settings = ();
    type Error = MesLoadError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<MesAsset, MesLoadError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_err(MesLoadError::Io)?;
        MesFile::parse(&bytes).map(MesAsset).map_err(MesLoadError::Mes)
    }

    fn extensions(&self) -> &[&str] {
        &["mes"]
    }
}
