//! The Brightness setting: a gamma curve applied to every finished frame, after the UI has been
//! drawn, so it covers the menus and the world alike. This is how the original worked too (it
//! changed the display gamma). Pure black and white stay as they are; darks and mid-tones get
//! lighter or darker.
//!
//! Brightness 0.5 (the default) is neutral. The curve is `colour ^ exponent` with
//! `exponent = 2 ^ ((0.5 - brightness) * 2 * MAX_STOPS)`: 0.0 gives 2^MAX_STOPS (darker), 1.0 gives
//! 2^-MAX_STOPS (lighter).

use bevy::asset::embedded_asset;
use bevy::core_pipeline::Core2d;
use bevy::core_pipeline::fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin};
use bevy::core_pipeline::upscaling::upscaling;
use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::BoxedSystem;
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::RenderApp;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;
use bevy::ui_render::ui_pass;

use crate::settings::GameSettings;

/// How far the slider's ends bend the curve: the exponent ranges over 2^±MAX_STOPS.
pub const MAX_STOPS: f32 = 1.0;

/// The gamma exponent for a Brightness value (`0.0..=1.0`, 0.5 neutral).
pub fn brightness_exponent(brightness: f32) -> f32 {
    let b = if brightness.is_finite() { brightness.clamp(0.0, 1.0) } else { 0.5 };
    2f32.powf((0.5 - b) * 2.0 * MAX_STOPS)
}

/// Per-camera brightness curve; kept in step with [`GameSettings`] on every 2D camera.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Debug, PartialEq)]
#[extract_app(RenderApp)]
#[extract_component_filter(With<Camera>)]
pub struct Brightness {
    pub exponent: f32,
    pub _padding: Vec3,
}

impl Default for Brightness {
    fn default() -> Self {
        Self { exponent: 1.0, _padding: Vec3::ZERO }
    }
}

impl FullscreenMaterial for Brightness {
    fn fragment_shader() -> ShaderRef {
        "embedded://arcanum_client/brightness.wgsl".into()
    }

    fn schedule() -> impl bevy::ecs::schedule::ScheduleLabel + Clone {
        Core2d
    }

    /// After the UI is drawn into the frame and before it is copied to the window.
    fn schedule_configs(system: ScheduleConfigs<BoxedSystem>) -> ScheduleConfigs<BoxedSystem> {
        system.after(ui_pass).before(upscaling)
    }
}

/// Applies Brightness to the screen. Needs the renderer; without one only the component is kept
/// up to date.
pub struct BrightnessPlugin;

impl Plugin for BrightnessPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "brightness.wgsl");
        app.add_plugins(FullscreenMaterialPlugin::<Brightness>::default())
            .add_systems(PostUpdate, sync_brightness);
    }
}

/// Puts the current curve on every 2D camera (new cameras included).
fn sync_brightness(
    mut commands: Commands,
    settings: Option<Res<GameSettings>>,
    mut cameras: Query<(Entity, Option<&mut Brightness>), With<Camera2d>>,
) {
    let exponent = brightness_exponent(settings.map_or(0.5, |s| s.brightness));
    for (entity, brightness) in &mut cameras {
        match brightness {
            Some(mut b) if b.exponent != exponent => b.exponent = exponent,
            Some(_) => {}
            None => {
                commands.entity(entity).insert(Brightness { exponent, ..default() });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_is_neutral_and_the_ends_bend_both_ways() {
        assert_eq!(brightness_exponent(0.5), 1.0);
        assert_eq!(brightness_exponent(0.0), 2.0, "darker: mid-grey 0.5 -> 0.25");
        assert_eq!(brightness_exponent(1.0), 0.5, "lighter: mid-grey 0.5 -> 0.71");
        assert!(brightness_exponent(0.25) > 1.0 && brightness_exponent(0.75) < 1.0);
        assert_eq!(brightness_exponent(9.0), 0.5, "clamped");
        assert_eq!(brightness_exponent(f32::NAN), 1.0);
    }
}
