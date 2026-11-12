//! The main menu screen: background art plus labels from `mes/MainMenu.mes`.

use bevy::camera::ScalingMode;
use bevy::prelude::*;

use crate::mes_loader::MesAsset;

/// The game was designed for 800x600. The camera keeps that area visible at any window size.
pub const VIRTUAL_WIDTH: f32 = 800.0;
pub const VIRTUAL_HEIGHT: f32 = 600.0;

pub const BACKGROUND_ART: &str = "art/interface/mainmenuback.art";
pub const MAIN_MENU_MES: &str = "mes/MainMenu.mes";

/// Message ids in `MainMenu.mes` for the main menu entries, top to bottom:
/// Single Player, MultiPlayer, Options, Credits, Exit Game.
pub const MENU_ENTRY_IDS: [u32; 5] = [10, 11, 12, 13, 14];

/// Centre of the labels, in virtual-screen pixels measured from the top-left of the art.
const LABEL_X: f32 = 412.0;
const LABEL_FIRST_Y: f32 = 145.0;
const LABEL_SPACING: f32 = 55.0;

#[derive(Component)]
pub struct MenuLabel(pub u32);

#[derive(Resource)]
struct MenuMessages(Handle<MesAsset>);

pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_menu)
            .add_systems(Update, spawn_labels.run_if(resource_exists::<MenuMessages>));
    }
}

fn setup_menu(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin { min_width: VIRTUAL_WIDTH, min_height: VIRTUAL_HEIGHT },
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn(Sprite::from_image(assets.load(BACKGROUND_ART)));
    commands.insert_resource(MenuMessages(assets.load(MAIN_MENU_MES)));
}

/// Waits for `MainMenu.mes` to load, then spawns one label per menu entry.
fn spawn_labels(mut commands: Commands, messages: Res<MenuMessages>, mes: Res<Assets<MesAsset>>) {
    let Some(mes) = mes.get(&messages.0) else { return };
    for (row, id) in MENU_ENTRY_IDS.into_iter().enumerate() {
        let Some(text) = mes.get(id) else {
            warn!("{MAIN_MENU_MES} has no message {id}");
            continue;
        };
        let y = LABEL_FIRST_Y + row as f32 * LABEL_SPACING;
        commands.spawn((
            MenuLabel(id),
            Text2d::new(text),
            TextFont::from_font_size(26.0),
            TextColor(Color::srgb(0.93, 0.80, 0.48)),
            // Art pixels run top-down from the corner; world space is y-up from the centre.
            Transform::from_xyz(LABEL_X - VIRTUAL_WIDTH / 2.0, VIRTUAL_HEIGHT / 2.0 - y, 1.0),
        ));
    }
    // Done: stop re-running this system.
    commands.remove_resource::<MenuMessages>();
}
