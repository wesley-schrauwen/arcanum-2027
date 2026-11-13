//! Fonts for menu and UI text.
//!
//! The original game ships only bitmap fonts (`.art` files such as `art/morph15font.art`).
//! Until those are decoded, a TrueType font placed in the override folder at
//! [`paths::fonts::SERIF`] is used; without one, text falls back to Bevy's built-in font.

use bevy::prelude::*;
use bevy::text::FontSize;

use crate::resource_pack::paths;
use crate::vfs_asset::VfsResource;

#[derive(Resource, Default, Clone, Debug)]
pub struct ArcanumFonts {
    /// The serif menu font, if one was found in the VFS.
    pub serif: Option<Handle<Font>>,
}

impl ArcanumFonts {
    /// A [`TextFont`] using the serif font (or Bevy's default) at `size` logical pixels on the
    /// 1080 baseline; `UiScale` scales it to the window.
    pub fn serif(&self, size: f32) -> TextFont {
        let font = TextFont { font_size: FontSize::Px(size), ..default() };
        match &self.serif {
            Some(handle) => font.with_font(handle.clone()),
            None => font,
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
    let available = vfs.is_some_and(|vfs| vfs.0.read().is_ok_and(|v| v.exists(paths::fonts::SERIF)));
    if available {
        fonts.serif = Some(assets.load(paths::fonts::SERIF));
    } else {
        info!("no {} in the VFS; using the built-in font", paths::fonts::SERIF);
    }
}
