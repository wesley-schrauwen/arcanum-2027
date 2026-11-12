//! The main menu screen: background art plus labels from `mes/MainMenu.mes`.
//!
//! World space and UI are both authored against a 1080-unit-tall baseline (see
//! [`crate::settings`]). The art keeps its aspect ratio and fills the height; wider windows
//! get black bars at the sides.

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::text::FontSize;

use crate::mes_loader::MesAsset;
use crate::settings::BASELINE_HEIGHT;

pub const BACKGROUND_ART: &str = "art/interface/mainmenuback.art";
pub const MAIN_MENU_MES: &str = "mes/MainMenu.mes";

/// Message ids in `MainMenu.mes` for the main menu entries, top to bottom:
/// Single Player, MultiPlayer, Options, Credits, Exit Game.
pub const MENU_ENTRY_IDS: [u32; 5] = [10, 11, 12, 13, 14];

// Label layout, measured on the original 800x600 art (pixels from its top-left corner), then
// scaled to the 1080 baseline. Replacement art with a different layout needs new numbers here.
const ART_HEIGHT: f32 = 600.0;
const ART_SCALE: f32 = BASELINE_HEIGHT / ART_HEIGHT;
/// Horizontal centre of the framed panel, relative to the middle of the art.
const LABEL_CENTER_OFFSET_X: f32 = 12.0 * ART_SCALE;
const LABEL_COLUMN_WIDTH: f32 = 400.0 * ART_SCALE;
const LABEL_ROW_HEIGHT: f32 = 55.0 * ART_SCALE;
/// Top of the first label row.
const LABEL_TOP: f32 = (145.0 - 55.0 / 2.0) * ART_SCALE;
const LABEL_FONT_SIZE: f32 = 26.0 * ART_SCALE;

#[derive(Component)]
pub struct MenuLabel(pub u32);

/// Marks the background sprite until its image has loaded and it has been fitted to the
/// baseline height.
#[derive(Component)]
pub struct FitToBaseline;

#[derive(Resource)]
struct MenuMessages(Handle<MesAsset>);

pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup_menu)
            .add_systems(
                Update,
                (fit_background, spawn_labels.run_if(resource_exists::<MenuMessages>)),
            );
    }
}

fn setup_menu(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn((
        Camera2d,
        // Always BASELINE_HEIGHT world units tall; the width follows the window's aspect ratio.
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: BASELINE_HEIGHT },
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn((Sprite::from_image(assets.load(BACKGROUND_ART)), FitToBaseline));
    commands.insert_resource(MenuMessages(assets.load(MAIN_MENU_MES)));
}

/// Scales the background so its height is the baseline height, preserving its aspect ratio.
fn fit_background(
    mut commands: Commands,
    images: Res<Assets<Image>>,
    mut sprites: Query<(Entity, &Sprite, &mut Transform), With<FitToBaseline>>,
) {
    for (entity, sprite, mut transform) in &mut sprites {
        let Some(image) = images.get(&sprite.image) else { continue };
        transform.scale = Vec3::splat(BASELINE_HEIGHT / image.height() as f32);
        commands.entity(entity).remove::<FitToBaseline>();
    }
}

/// True once the labels exist and the background has been fitted (or has no image to wait for).
pub fn menu_ready(labels: Query<(), With<MenuLabel>>, pending: Query<(), With<FitToBaseline>>) -> bool {
    !labels.is_empty() && pending.is_empty()
}

/// Waits for `MainMenu.mes` to load, then spawns the label column as a UI hierarchy.
fn spawn_labels(
    mut commands: Commands,
    messages: Res<MenuMessages>,
    mes: Res<Assets<MesAsset>>,
    assets: Res<AssetServer>,
) {
    let Some(mes) = mes.get(&messages.0) else {
        if let Some(bevy::asset::LoadState::Failed(err)) = assets.get_load_state(&messages.0) {
            error!("cannot load {MAIN_MENU_MES}: {err}");
            commands.remove_resource::<MenuMessages>();
        }
        return;
    };

    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(LABEL_TOP),
            left: Val::Percent(50.0),
            // Centre the column on the panel, which sits slightly right of the art's middle.
            margin: UiRect::left(Val::Px(LABEL_CENTER_OFFSET_X - LABEL_COLUMN_WIDTH / 2.0)),
            width: Val::Px(LABEL_COLUMN_WIDTH),
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|column| {
            for id in MENU_ENTRY_IDS {
                let Some(text) = mes.get(id) else {
                    warn!("{MAIN_MENU_MES} has no message {id}");
                    continue;
                };
                column
                    .spawn(Node {
                        height: Val::Px(LABEL_ROW_HEIGHT),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_child((
                        MenuLabel(id),
                        Text::new(text),
                        TextFont { font_size: FontSize::Px(LABEL_FONT_SIZE), ..default() },
                        TextColor(Color::srgb(0.93, 0.80, 0.48)),
                    ));
            }
        });

    // Done: stop re-running this system.
    commands.remove_resource::<MenuMessages>();
}
