//! Small reusable UI behaviours shared by the menu screens.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::Pressed;

use crate::art_loader::ArtFrames;
use crate::audio::MenuSounds;

/// Text that switches to `hover` while the pointer is over it (needs [`Hovered`] and
/// [`TextColor`]), and plays the hover sound when the pointer arrives.
#[derive(Component, Debug, Clone, Copy)]
pub struct HoverText {
    pub normal: Color,
    pub hover: Color,
}

/// Keeps a [`HoverText`] in its hover colour, e.g. the active tab.
#[derive(Component, Debug)]
pub struct Selected;

/// An image button drawn from an `.art` file's frames: `normal`, `pressed` and `hover` are
/// frame indices. Needs [`Hovered`] and an [`ImageNode`].
#[derive(Component, Debug, Clone)]
pub struct FrameButton {
    pub frames: Handle<ArtFrames>,
    pub normal: usize,
    pub pressed: usize,
    pub hover: usize,
}

impl FrameButton {
    /// The frame layout of the options arrow buttons: normal, pressed, hover.
    pub fn arrow(frames: Handle<ArtFrames>) -> Self {
        Self { frames, normal: 0, pressed: 1, hover: 2 }
    }

    fn frame_for(&self, hovered: bool, pressed: bool) -> usize {
        if pressed {
            self.pressed
        } else if hovered {
            self.hover
        } else {
            self.normal
        }
    }
}

pub struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (colour_hover_text, play_hover_sound, show_button_frame));
    }
}

fn colour_hover_text(mut texts: Query<(&HoverText, &Hovered, Has<Selected>, &mut TextColor)>) {
    for (colours, hovered, selected, mut text_color) in &mut texts {
        let wanted = if hovered.get() || selected { colours.hover } else { colours.normal };
        if text_color.0 != wanted {
            text_color.0 = wanted;
        }
    }
}

fn play_hover_sound(
    mut commands: Commands,
    hovered: Query<&Hovered, (Changed<Hovered>, With<HoverText>)>,
    sounds: Option<Res<MenuSounds>>,
) {
    let Some(sounds) = sounds else { return };
    for hovered in &hovered {
        if hovered.get() {
            sounds.play_hover(&mut commands);
        }
    }
}

fn show_button_frame(
    frames: Res<Assets<ArtFrames>>,
    mut buttons: Query<(&FrameButton, &Hovered, Has<Pressed>, &mut ImageNode)>,
) {
    for (button, hovered, pressed, mut image) in &mut buttons {
        let Some(set) = frames.get(&button.frames) else { continue };
        let Some(frame) = set.frame(button.frame_for(hovered.get(), pressed)) else { continue };
        if image.image != *frame {
            image.image = frame.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_choice_prefers_pressed_then_hover() {
        let b = FrameButton::arrow(Handle::default());
        assert_eq!(b.frame_for(false, false), 0);
        assert_eq!(b.frame_for(true, false), 2);
        assert_eq!(b.frame_for(true, true), 1);
        assert_eq!(b.frame_for(false, true), 1);
    }
}
