//! Bevy [`AssetLoader`] that turns `.art` sprites into [`Image`]s.

use arcanum_formats::art::{ArtError, ArtFile};
use bevy::asset::{AssetLoader, LoadContext, RenderAssetUsages, io::Reader};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Decodes frame 0 of rotation 0 using the file's first palette.
#[derive(Default, TypePath)]
pub struct ArtAssetLoader;

#[derive(Debug)]
pub enum ArtLoadError {
    Io(std::io::Error),
    Art(ArtError),
}

impl std::fmt::Display for ArtLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "could not read .art file: {e}"),
            Self::Art(e) => write!(f, "invalid .art file: {e}"),
        }
    }
}

impl std::error::Error for ArtLoadError {}

impl AssetLoader for ArtAssetLoader {
    type Asset = Image;
    type Settings = ();
    type Error = ArtLoadError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Image, ArtLoadError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_err(ArtLoadError::Io)?;
        let (rgba, (width, height)) = ArtFile::parse(&bytes)
            .and_then(|art| art.decode_frame0())
            .map_err(ArtLoadError::Art)?;
        Ok(Image::new(
            Extent3d { width, height, depth_or_array_layers: 1 },
            TextureDimension::D2,
            rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        ))
    }

    fn extensions(&self) -> &[&str] {
        &["art"]
    }
}
