//! The main menu: background art and five buttons labelled from `mes/MainMenu.mes`.
//!
//! World space and UI are both authored against a 1080-unit-tall baseline (see
//! [`crate::settings`]). The art keeps its aspect ratio and fills the height; wider windows
//! get black bars at the sides.
//!
//! Buttons use Bevy's headless button widget: `ActivateOnPress` makes a press send
//! [`Activate`], and the picking backend keeps [`Hovered`] up to date.

use bevy::camera::ScalingMode;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, ActivateOnPress, Button};

use crate::audio::MenuSounds;
use crate::fonts::ArcanumFonts;
use crate::mes_loader::MesAsset;
use crate::resource_pack::paths::{backgrounds, text};
use crate::settings::BASELINE_HEIGHT;

/// Which screen is showing.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    SinglePlayer,
    Options,
    Credits,
}

/// What a main menu button does.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    SinglePlayer,
    MultiPlayer,
    Options,
    Credits,
    ExitGame,
}

/// Main menu entries, top to bottom, with their message ids in `MainMenu.mes`.
pub const MENU_ENTRIES: [(u32, MenuAction); 5] = [
    (10, MenuAction::SinglePlayer),
    (11, MenuAction::MultiPlayer),
    (12, MenuAction::Options),
    (13, MenuAction::Credits),
    (14, MenuAction::ExitGame),
];

/// Message ids of [`MENU_ENTRIES`], in order.
pub const MENU_ENTRY_IDS: [u32; 5] = [10, 11, 12, 13, 14];

/// Classic dimmed gold for menu text, and the brighter gold for the hovered entry.
pub const TEXT_COLOR: Color = Color::srgb(0.75, 0.65, 0.45);
pub const HOVER_COLOR: Color = Color::srgb(1.0, 0.9, 0.7);

// Layout, measured on the original 800x600 art (pixels from its top-left corner), then scaled to
// the 1080 baseline. Replacement art with a different layout needs new numbers here.
const ART_HEIGHT: f32 = 600.0;
const ART_SCALE: f32 = BASELINE_HEIGHT / ART_HEIGHT;
/// Horizontal centre of the framed panel, relative to the middle of the art.
pub(crate) const PANEL_CENTER_OFFSET_X: f32 = 12.0 * ART_SCALE;
pub(crate) const PANEL_WIDTH: f32 = 400.0 * ART_SCALE;
const ROW_HEIGHT: f32 = 55.0 * ART_SCALE;
/// Top of the first row.
pub(crate) const PANEL_TOP: f32 = (145.0 - 55.0 / 2.0) * ART_SCALE;
pub(crate) const FONT_SIZE: f32 = 26.0 * ART_SCALE;

/// The text node inside a menu button, tagged with its message id.
#[derive(Component)]
pub struct MenuLabel(pub u32);

/// Root of the main menu's UI; despawned when leaving [`AppState::MainMenu`].
#[derive(Component)]
pub struct OnMainMenu;

/// Marks the background sprite until its image has loaded and it has been fitted to the
/// baseline height.
#[derive(Component)]
pub struct FitToBaseline;

/// `MainMenu.mes`, kept loaded for every menu screen.
#[derive(Resource)]
pub struct MenuText(pub Handle<MesAsset>);

/// Present while the main menu buttons still need spawning (waiting for `MainMenu.mes`).
#[derive(Resource)]
struct PendingMenu;

pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup_menu)
            .add_systems(OnEnter(AppState::MainMenu), |mut commands: Commands| {
                commands.insert_resource(PendingMenu)
            })
            .add_systems(
                Update,
                (
                    fit_background,
                    spawn_menu.run_if(in_state(AppState::MainMenu)).run_if(resource_exists::<PendingMenu>),
                    highlight_hovered,
                ),
            )
            .add_observer(on_activate);
    }
}

/// Camera, background and menu text. These stay for every menu screen.
fn setup_menu(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn((
        Camera2d,
        // Always BASELINE_HEIGHT world units tall; the width follows the window's aspect ratio.
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: BASELINE_HEIGHT },
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn((Sprite::from_image(assets.load(backgrounds::MAIN_MENU)), FitToBaseline));
    commands.insert_resource(MenuText(assets.load(text::MAIN_MENU)));
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

/// True once the menu labels exist and the background has been fitted.
pub fn menu_ready(labels: Query<(), With<MenuLabel>>, pending: Query<(), With<FitToBaseline>>) -> bool {
    !labels.is_empty() && pending.is_empty()
}

/// A node covering the menu panel on the background art, for screens to put content in.
pub(crate) fn panel_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        top: Val::Px(PANEL_TOP),
        left: Val::Percent(50.0),
        // Centre on the panel, which sits slightly right of the art's middle.
        margin: UiRect::left(Val::Px(PANEL_CENTER_OFFSET_X - PANEL_WIDTH / 2.0)),
        width: Val::Px(PANEL_WIDTH),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Stretch,
        ..default()
    }
}

/// Spawns the button column once `MainMenu.mes` has loaded.
fn spawn_menu(
    mut commands: Commands,
    menu_text: Res<MenuText>,
    mes: Res<Assets<MesAsset>>,
    assets: Res<AssetServer>,
    fonts: Option<Res<ArcanumFonts>>,
) {
    let Some(mes) = mes.get(&menu_text.0) else {
        if let Some(bevy::asset::LoadState::Failed(err)) = assets.get_load_state(&menu_text.0) {
            error!("cannot load {}: {err}", text::MAIN_MENU);
            commands.remove_resource::<PendingMenu>();
        }
        return;
    };
    let fonts = fonts.map(|f| f.clone()).unwrap_or_default();

    commands
        .spawn((OnMainMenu, DespawnOnExit(AppState::MainMenu), panel_node()))
        .with_children(|column| {
            for (id, action) in MENU_ENTRIES {
                let Some(label) = mes.get(id) else {
                    warn!("{} has no message {id}", text::MAIN_MENU);
                    continue;
                };
                column
                    .spawn((
                        action,
                        Button,
                        ActivateOnPress,
                        Hovered::default(),
                        Node {
                            height: Val::Px(ROW_HEIGHT),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                    ))
                    .with_child((MenuLabel(id), Text::new(label), fonts.serif(FONT_SIZE), TextColor(TEXT_COLOR)));
            }
        });
    commands.remove_resource::<PendingMenu>();
}

/// Brightens a button's text while hovered and plays the hover sound when the pointer enters.
fn highlight_hovered(
    mut commands: Commands,
    buttons: Query<(&Hovered, &Children), (Changed<Hovered>, With<MenuAction>)>,
    mut colors: Query<&mut TextColor>,
    sounds: Option<Res<MenuSounds>>,
) {
    for (hovered, children) in &buttons {
        let color = if hovered.get() { HOVER_COLOR } else { TEXT_COLOR };
        for child in children {
            if let Ok(mut text_color) = colors.get_mut(*child) {
                text_color.0 = color;
            }
        }
        if hovered.get()
            && let Some(sounds) = &sounds
        {
            sounds.play_hover(&mut commands);
        }
    }
}

/// Runs a menu button's action when it is pressed.
fn on_activate(
    activate: On<Activate>,
    actions: Query<&MenuAction>,
    mut commands: Commands,
    sounds: Option<Res<MenuSounds>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(action) = actions.get(activate.entity) else { return };
    if let Some(sounds) = &sounds {
        sounds.play_click(&mut commands);
    }
    match action {
        MenuAction::SinglePlayer => next_state.set(AppState::SinglePlayer),
        MenuAction::Options => next_state.set(AppState::Options),
        MenuAction::Credits => next_state.set(AppState::Credits),
        MenuAction::MultiPlayer => info!("multiplayer is not implemented"),
        MenuAction::ExitGame => {
            exit.write(AppExit::Success);
        }
    }
}
