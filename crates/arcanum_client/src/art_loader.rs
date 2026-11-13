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
        Ok(rgba_image(width, height, rgba))
    }

    fn extensions(&self) -> &[&str] {
        // Extension matching is case-sensitive and the game mixes `.art` and `.ART`.
        &["art", "ART", "Art"]
    }
}

fn rgba_image(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    Image::new(
        Extent3d { width, height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Every frame of rotation 0 of an `.art` file, each as its own [`Image`] (sub-assets labelled
/// `frame0`, `frame1`, ...). Used for multi-state art such as buttons.
#[derive(Asset, TypePath, Debug, Clone)]
pub struct ArtFrames {
    pub frames: Vec<Handle<Image>>,
}

impl ArtFrames {
    /// Frame `index`, or the last frame if there are fewer.
    pub fn frame(&self, index: usize) -> Option<&Handle<Image>> {
        self.frames.get(index).or(self.frames.last())
    }
}

/// Loads `.art` files as [`ArtFrames`]. Shares the extension with the other `.art` loaders;
/// Bevy picks by the requested asset type.
#[derive(Default, TypePath)]
pub struct ArtFramesLoader;

impl AssetLoader for ArtFramesLoader {
    type Asset = ArtFrames;
    type Settings = ();
    type Error = ArtLoadError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<ArtFrames, ArtLoadError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_err(ArtLoadError::Io)?;
        let art = ArtFile::parse(&bytes).map_err(ArtLoadError::Art)?;
        let palette = art.default_palette().map_err(ArtLoadError::Art)?;
        let mut frames = Vec::with_capacity(art.frame_count);
        for index in 0..art.frame_count {
            let frame = art.decode_frame(0, index, palette).map_err(ArtLoadError::Art)?;
            let image = rgba_image(frame.info.width, frame.info.height, frame.rgba);
            frames.push(load_context.add_labeled_asset(format!("frame{index}"), image));
        }
        if frames.is_empty() {
            return Err(ArtLoadError::Art(ArtError::NoFrames));
        }
        Ok(ArtFrames { frames })
    }

    fn extensions(&self) -> &[&str] {
        // Extension matching is case-sensitive and the game mixes `.art` and `.ART`.
        &["art", "ART", "Art"]
    }
}
