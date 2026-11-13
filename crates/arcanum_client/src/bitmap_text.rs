//! UI text drawn with Arcanum's bitmap fonts.
//!
//! [`BitmapText`] goes on a UI node. Once its font has loaded, the text is rasterised into a
//! white image with coverage in the alpha channel, shown with an [`ImageNode`] tinted by the
//! entity's [`TextColor`]. Changing the colour only retints; changing the text re-rasterises.
//! The node is sized `pixels * scale`, so `scale` is the size relative to the original game's
//! 800x600 screen mapped onto the 1080 baseline.
//!
//! With `shadow` set, a black copy of the text is drawn one font pixel down and to the right
//! underneath it, like the original menus. The shadow is black, so tinting leaves it black.
//!
//! If the font fails to load, the entity falls back to ordinary [`Text`] with Bevy's built-in
//! font so the words still appear.

pub use arcanum_formats::font::Align;
use arcanum_formats::font::{BitmapFont, FontError, RenderedText};
use bevy::asset::{AssetLoader, LoadContext, LoadState, RenderAssetUsages, io::Reader};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::text::FontSize;

/// A decoded bitmap font.
#[derive(Asset, TypePath, Debug, Clone)]
pub struct BitmapFontAsset(pub BitmapFont);

/// Loads `.art` files as bitmap fonts. Shares the extension with the sprite loader; Bevy picks
/// by the requested asset type (`Handle<BitmapFontAsset>` vs `Handle<Image>`).
#[derive(Default, TypePath)]
pub struct BitmapFontLoader;

#[derive(Debug)]
pub enum BitmapFontLoadError {
    Io(std::io::Error),
    Font(FontError),
}

impl std::fmt::Display for BitmapFontLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "could not read font: {e}"),
            Self::Font(e) => write!(f, "invalid font: {e}"),
        }
    }
}

impl std::error::Error for BitmapFontLoadError {}

impl AssetLoader for BitmapFontLoader {
    type Asset = BitmapFontAsset;
    type Settings = ();
    type Error = BitmapFontLoadError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<BitmapFontAsset, BitmapFontLoadError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_err(BitmapFontLoadError::Io)?;
        BitmapFont::parse(&bytes).map(BitmapFontAsset).map_err(BitmapFontLoadError::Font)
    }

    fn extensions(&self) -> &[&str] {
        &["art", "ART", "Art"]
    }
}

/// Text in a bitmap font. Tint it with [`TextColor`].
///
/// Newlines (real or the game's escaped `\n`) start new lines and leading tabs indent; with
/// `max_width` the text also wraps between words to fit that width.
#[derive(Component, Debug, Clone)]
#[require(Node)]
pub struct BitmapText {
    pub text: String,
    pub font: Handle<BitmapFontAsset>,
    /// Logical pixels per font pixel.
    pub scale: f32,
    /// Draw a drop shadow one font pixel down and right.
    pub shadow: bool,
    /// Wrap to this width, in logical pixels. `None`: lines are as long as their text.
    pub max_width: Option<f32>,
    pub align: Align,
}

impl BitmapText {
    /// One block of text in `font`, unwrapped, left-aligned, with a shadow.
    pub fn new(text: impl Into<String>, font: Handle<BitmapFontAsset>, scale: f32) -> Self {
        Self { text: text.into(), font, scale, shadow: true, max_width: None, align: Align::Left }
    }

    /// Wraps to `width` logical pixels.
    pub fn wrapped(mut self, width: f32, align: Align) -> Self {
        self.max_width = Some(width);
        self.align = align;
        self
    }

    /// The wrap width in font pixels.
    fn wrap_in_font_pixels(&self) -> Option<u32> {
        self.max_width.map(|w| (w / self.scale).floor().max(1.0) as u32)
    }
}

/// Opacity of the drop shadow where the text is fully opaque.
pub const SHADOW_OPACITY: f32 = 0.8;

/// White text over an optional black shadow, as straight-alpha RGBA8. Returns the image size,
/// one pixel larger in each direction when there is a shadow.
pub fn compose_rgba(text: &RenderedText, shadow: bool) -> (u32, u32, Vec<u8>) {
    let pad = u32::from(shadow);
    let (w, h) = (text.width + pad, text.height + pad);
    let coverage = |x: i64, y: i64| -> f32 {
        if x < 0 || y < 0 || x >= text.width as i64 || y >= text.height as i64 {
            0.0
        } else {
            text.alpha[y as usize * text.width as usize + x as usize] as f32 / 255.0
        }
    };
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let a_text = coverage(x, y);
            let a_shadow = if shadow { coverage(x - 1, y - 1) * SHADOW_OPACITY } else { 0.0 };
            // White text "over" black shadow.
            let a = a_text + a_shadow * (1.0 - a_text);
            let grey = if a > 0.0 { a_text / a } else { 1.0 };
            let g = (grey * 255.0).round() as u8;
            rgba.extend_from_slice(&[g, g, g, (a * 255.0).round() as u8]);
        }
    }
    (w, h, rgba)
}

/// Set once the text has been rasterised; removed again when [`BitmapText`] changes.
#[derive(Component, Debug)]
pub struct BitmapTextRendered;

/// Registers the font asset, its loader and the text systems. Add after `DefaultPlugins`.
pub struct BitmapTextPlugin;

impl Plugin for BitmapTextPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<BitmapFontAsset>()
            .register_asset_loader(BitmapFontLoader)
            .add_systems(PostUpdate, (fall_back_to_plain_text, render_bitmap_text, tint_bitmap_text).chain().before(bevy::ui::UiSystems::Layout));
    }
}

fn render_bitmap_text(
    mut commands: Commands,
    fonts: Res<Assets<BitmapFontAsset>>,
    mut images: ResMut<Assets<Image>>,
    mut texts: Query<
        (Entity, &BitmapText, Option<&TextColor>, &mut Node),
        Or<(Changed<BitmapText>, Without<BitmapTextRendered>)>,
    >,
) {
    for (entity, text, color, mut node) in &mut texts {
        let Some(font) = fonts.get(&text.font) else {
            // Not loaded yet: retry next frame.
            commands.entity(entity).remove::<BitmapTextRendered>();
            continue;
        };
        let wrap = text.wrap_in_font_pixels();
        let rendered = font.0.render_block(&text.text, wrap, text.align);
        let mut entity_commands = commands.entity(entity);
        if rendered.width == 0 {
            node.width = Val::Px(0.0);
            node.height = Val::Px(rendered.height as f32 * text.scale);
            entity_commands.remove::<ImageNode>();
        } else {
            let (width, height, rgba) = compose_rgba(&rendered, text.shadow);
            node.width = Val::Px(width as f32 * text.scale);
            node.height = Val::Px(height as f32 * text.scale);
            let image = Image::new(
                Extent3d { width, height, depth_or_array_layers: 1 },
                TextureDimension::D2,
                rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            let tint = color.map_or(Color::WHITE, |c| c.0);
            entity_commands.insert(ImageNode::new(images.add(image)).with_color(tint));
        }
        entity_commands.insert(BitmapTextRendered);
    }
}

/// Keeps the image tint in step with [`TextColor`] (e.g. hover highlighting).
fn tint_bitmap_text(mut texts: Query<(&TextColor, &mut ImageNode), (With<BitmapText>, Changed<TextColor>)>) {
    for (color, mut image) in &mut texts {
        image.color = color.0;
    }
}

/// Swaps text whose font failed to load for plain [`Text`] in Bevy's built-in font.
fn fall_back_to_plain_text(
    mut commands: Commands,
    texts: Query<(Entity, &BitmapText), Without<BitmapTextRendered>>,
    assets: Res<AssetServer>,
) {
    for (entity, text) in &texts {
        if let Some(LoadState::Failed(err)) = assets.get_load_state(&text.font) {
            warn!("bitmap font failed to load ({err}); using the built-in font for {:?}", text.text);
            // Roughly the bitmap fonts' cap height relative to their line height.
            let size = text.scale * 24.0;
            commands
                .entity(entity)
                .remove::<(BitmapText, ImageNode)>()
                .insert((Text::new(text.text.clone()), TextFont { font_size: FontSize::Px(size), ..default() }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(width: u32, height: u32, alpha: &[u8]) -> RenderedText {
        RenderedText { width, height, alpha: alpha.to_vec() }
    }

    #[test]
    fn without_shadow_is_white_with_coverage_alpha() {
        let (w, h, rgba) = compose_rgba(&text(2, 1, &[255, 51]), false);
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgba, [255, 255, 255, 255, 255, 255, 255, 51]);
    }

    #[test]
    fn shadow_is_black_offset_down_right() {
        // One solid pixel: the shadow lands at (1, 1) and the image grows to 2x2.
        let (w, h, rgba) = compose_rgba(&text(1, 1, &[255]), true);
        assert_eq!((w, h), (2, 2));
        let px = |x: usize, y: usize| &rgba[(y * 2 + x) * 4..(y * 2 + x) * 4 + 4];
        assert_eq!(px(0, 0), [255, 255, 255, 255]);
        assert_eq!(px(1, 0), [255, 255, 255, 0]);
        assert_eq!(px(0, 1), [255, 255, 255, 0]);
        assert_eq!(px(1, 1), [0, 0, 0, (SHADOW_OPACITY * 255.0).round() as u8]);
    }

    #[test]
    fn text_covers_its_own_shadow() {
        // Two solid pixels on a diagonal: the second sits exactly on the first one's shadow.
        let (w, _, rgba) = compose_rgba(&text(2, 2, &[255, 0, 0, 255]), true);
        let i = (w as usize + 1) * 4; // pixel (1, 1)
        assert_eq!(&rgba[i..i + 4], [255, 255, 255, 255]);
    }

    #[test]
    fn partial_text_over_shadow_blends_toward_black() {
        // Half-covered text pixel on top of a full shadow.
        let (w, _, rgba) = compose_rgba(&text(2, 2, &[255, 0, 0, 128]), true);
        let i = (w as usize + 1) * 4; // pixel (1, 1)
        let p = &rgba[i..i + 4];
        assert!(p[0] > 100 && p[0] < 200, "grey between text and shadow: {p:?}");
        assert!(p[3] >= 225, "nearly opaque: {p:?}");
    }
}
