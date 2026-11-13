//! Fonts for menu and UI text.
//!
//! In order of preference:
//! 1. a TrueType font in the override folder at [`paths::fonts::SERIF`] (for modders);
//! 2. the game's own bitmap fonts ([`paths::fonts::LARGE`], [`paths::fonts::SMALL`]), drawn
//!    with [`BitmapText`];
//! 3. Bevy's built-in font.

use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;
use bevy::text::FontSize;

use crate::bitmap_text::{BitmapFontAsset, BitmapText};
use crate::resource_pack::paths;
use crate::settings::BASELINE_HEIGHT;
use crate::vfs_asset::VfsResource;

/// Scale from the original 800x600 screen to the 1080 baseline. Bitmap fonts are drawn at this
/// scale so text keeps its original size relative to the original art.
pub const ORIGINAL_SCALE: f32 = BASELINE_HEIGHT / 600.0;

/// Which of the UI fonts to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiFont {
    /// Menu entries and titles (`Morph30Font`).
    Large,
    /// Hints and small print (`morph15font`).
    Small,
}

impl UiFont {
    /// TrueType size for this font on the 1080 baseline, used for the override and built-in fonts.
    pub fn ttf_size(self) -> f32 {
        match self {
            Self::Large => 26.0 * ORIGINAL_SCALE,
            Self::Small => 15.0 * ORIGINAL_SCALE,
        }
    }
}

#[derive(Resource, Default, Clone, Debug)]
pub struct ArcanumFonts {
    /// TrueType override font, if one is in the VFS. Takes priority over the bitmap fonts.
    pub serif: Option<Handle<Font>>,
    /// The game's large bitmap font, if present.
    pub large: Option<Handle<BitmapFontAsset>>,
    /// The game's small bitmap font, if present.
    pub small: Option<Handle<BitmapFontAsset>>,
}

impl ArcanumFonts {
    /// A [`TextFont`] using the override TTF (or Bevy's default) at `size` logical pixels on
    /// the 1080 baseline; `UiScale` scales it to the window.
    pub fn serif(&self, size: f32) -> TextFont {
        let font = TextFont { font_size: FontSize::Px(size), ..default() };
        match &self.serif {
            Some(handle) => font.with_font(handle.clone()),
            None => font,
        }
    }

    fn bitmap(&self, font: UiFont) -> Option<&Handle<BitmapFontAsset>> {
        match font {
            UiFont::Large => self.large.as_ref(),
            UiFont::Small => self.small.as_ref(),
        }
    }

    /// Adds `text` in `font` to `entity`, using the best available font. Tint it with
    /// [`TextColor`].
    pub fn insert_text(&self, entity: &mut EntityCommands, text: &str, font: UiFont) {
        if self.serif.is_none()
            && let Some(bitmap) = self.bitmap(font)
        {
            entity.insert(BitmapText { text: text.to_string(), font: bitmap.clone(), scale: ORIGINAL_SCALE, shadow: true });
        } else {
            entity.insert((Text::new(text), self.serif(font.ttf_size())));
        }
    }
}

pub struct FontsPlugin;

impl Plugin for FontsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ArcanumFonts>().add_systems(PreStartup, load_fonts);
    }
}

fn load_fonts(mut fonts: ResMut<ArcanumFonts>, vfs: Option<Res<VfsResource>>, assets: Res<AssetServer>) {
    let exists = |path: &str| vfs.as_ref().is_some_and(|vfs| vfs.0.read().is_ok_and(|v| v.exists(path)));
    if exists(paths::fonts::SERIF) {
        fonts.serif = Some(assets.load(paths::fonts::SERIF));
        info!("using override font {}", paths::fonts::SERIF);
    }
    if exists(paths::fonts::LARGE) {
        fonts.large = Some(assets.load(paths::fonts::LARGE));
    }
    if exists(paths::fonts::SMALL) {
        fonts.small = Some(assets.load(paths::fonts::SMALL));
    }
    if fonts.serif.is_none() && (fonts.large.is_none() || fonts.small.is_none()) {
        warn!("game bitmap fonts missing from the VFS; using the built-in font where needed");
    }
}
