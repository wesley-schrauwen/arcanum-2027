//! The frame shared by the original's 800x600 menu screens (Load Game, New Game, character
//! creation): an 800x41 top bar, an 800x400 background for the screen itself and an 800x159
//! bottom bar with round Back and Next buttons and a text box for hover help.
//!
//! Positions come from the original's screen table in Arcanum.exe (0x5c4560). Everything is in
//! original pixels and scaled by [`ORIGINAL_SCALE`] to the 1080 baseline, centred in the window.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{ActivateOnPress, Button};

use crate::art_loader::ArtFrames;
use crate::bitmap_text::BitmapText;
use crate::fonts::{ArcanumFonts, ORIGINAL_SCALE, UiFont};
use crate::main_menu::MenuText;
use crate::mes_loader::MesAsset;
use crate::resource_pack::paths::interface;
use crate::widgets::{FrameButton, WidgetsPlugin};

pub const TOP_BAR_HEIGHT: f32 = 41.0;
pub const BACKGROUND_HEIGHT: f32 = 400.0;
pub const BOTTOM_BAR_TOP: f32 = TOP_BAR_HEIGHT + BACKGROUND_HEIGHT;
/// Round bottom-bar buttons (`BackBut`, `NextBut`).
pub const BACK_AT: Vec2 = Vec2::new(69.0, BOTTOM_BAR_TOP + 55.0);
pub const NEXT_AT: Vec2 = Vec2::new(675.0, BOTTOM_BAR_TOP + 55.0);
pub const ROUND_BUTTON: Vec2 = Vec2::new(61.0, 61.0);
/// The bottom bar's text box.
pub const HELP_BOX: Rect = Rect { min: Vec2::new(206.0, BOTTOM_BAR_TOP + 58.0), max: Vec2::new(594.0, BOTTOM_BAR_TOP + 150.0) };

/// Hover help and other light text in the frame.
pub const INFO_COLOR: Color = Color::srgb_u8(236, 220, 180);

/// `MainMenu.mes` id of a button's hover help, and its English fallback.
#[derive(Component, Debug, Clone, Copy)]
pub struct HoverHelp(pub u32, pub &'static str);

/// The bottom bar's hover help text.
#[derive(Component)]
pub struct HoverHelpText;

/// The frame's art, loaded at startup so screens open instantly.
#[derive(Resource, Clone)]
pub struct WindowArt {
    pub top_bar: Handle<Image>,
    pub bottom_bar: Handle<Image>,
    pub back: Handle<ArtFrames>,
    pub next: Handle<ArtFrames>,
}

pub struct MenuWindowPlugin;

impl Plugin for MenuWindowPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<WidgetsPlugin>() {
            app.add_plugins(WidgetsPlugin);
        }
        app.add_systems(Startup, load_art).add_systems(Update, show_hover_help);
    }
}

fn load_art(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(WindowArt {
        top_bar: assets.load(interface::TOP_BAR),
        bottom_bar: assets.load(interface::BOTTOM_BAR),
        back: assets.load(interface::BACK_BUTTON),
        next: assets.load(interface::NEXT_BUTTON),
    });
}

/// `v` original pixels.
pub fn px(v: f32) -> Val {
    Val::Px(v * ORIGINAL_SCALE)
}

/// An absolutely positioned node at `rect` (original pixels).
pub fn at(rect: Rect) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(rect.min.x),
        top: px(rect.min.y),
        width: px(rect.width()),
        height: px(rect.height()),
        ..default()
    }
}

pub fn rect(at: Vec2, size: Vec2) -> Rect {
    Rect::from_corners(at, at + size)
}

/// The 800x600 screen, centred, black behind any transparent parts of the art.
pub fn screen_root() -> impl Bundle {
    (
        BackgroundColor(Color::BLACK),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(0.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(px(-400.0)),
            width: px(800.0),
            height: px(600.0),
            ..default()
        },
    )
}

/// A button drawn from a three-frame `.art` file (normal, pressed, hover).
pub fn image_button(action: impl Bundle, help: HoverHelp, frames: &Handle<ArtFrames>, rect: Rect) -> impl Bundle {
    (
        action,
        help,
        Button,
        ActivateOnPress,
        Hovered::default(),
        FrameButton::arrow(frames.clone()),
        ImageNode::default(),
        at(rect),
    )
}

/// Spawns the bars, `background` between them, the round Back and Next buttons (carrying
/// `back` and `next`) and the hover help box.
pub fn spawn_frame(
    screen: &mut ChildSpawnerCommands,
    art: &WindowArt,
    fonts: &ArcanumFonts,
    background: Handle<Image>,
    back: impl Bundle,
    next: (impl Bundle, HoverHelp),
) {
    screen.spawn((ImageNode::new(art.top_bar.clone()), at(rect(Vec2::ZERO, Vec2::new(800.0, TOP_BAR_HEIGHT)))));
    screen.spawn((ImageNode::new(background), at(rect(Vec2::new(0.0, TOP_BAR_HEIGHT), Vec2::new(800.0, BACKGROUND_HEIGHT)))));
    screen.spawn((ImageNode::new(art.bottom_bar.clone()), at(rect(Vec2::new(0.0, BOTTOM_BAR_TOP), Vec2::new(800.0, 159.0)))));
    screen.spawn(image_button(back, HoverHelp(7002, "Previous Screen"), &art.back, rect(BACK_AT, ROUND_BUTTON)));
    screen.spawn(image_button(next.0, next.1, &art.next, rect(NEXT_AT, ROUND_BUTTON)));
    screen
        .spawn(Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..at(HELP_BOX) })
        .with_children(|help_box| {
            let mut help = help_box.spawn((HoverHelpText, TextColor(INFO_COLOR)));
            fonts.insert_text(&mut help, "", UiFont::Small);
        });
}

/// Shows the hovered button's help in the bottom bar.
fn show_hover_help(
    buttons: Query<(&HoverHelp, &Hovered)>,
    menu_text: Option<Res<MenuText>>,
    mes: Res<Assets<MesAsset>>,
    mut help: Query<(Option<&mut BitmapText>, Option<&mut Text>), With<HoverHelpText>>,
) {
    if help.is_empty() {
        return;
    }
    let wanted = buttons
        .iter()
        .find(|(_, hovered)| hovered.get())
        .map(|(HoverHelp(id, fallback), _)| {
            menu_text.as_ref().and_then(|t| mes.get(&t.0)).and_then(|m| m.get(*id)).unwrap_or(fallback).to_string()
        })
        .unwrap_or_default();
    for (bitmap, text) in &mut help {
        if let Some(mut bitmap) = bitmap {
            if bitmap.text != wanted {
                bitmap.text = wanted.clone();
            }
        } else if let Some(mut text) = text
            && text.0 != wanted
        {
            text.0 = wanted.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_stacks_into_the_original_screen() {
        assert_eq!(BOTTOM_BAR_TOP + 159.0, 600.0);
        for at in [BACK_AT, NEXT_AT] {
            assert!(at.x + ROUND_BUTTON.x <= 800.0 && at.y + ROUND_BUTTON.y <= 600.0, "{at:?}");
        }
    }
}
