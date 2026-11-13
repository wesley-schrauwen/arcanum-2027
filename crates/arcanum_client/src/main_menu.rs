//! The main menu and the Single Player menu: background art and a column of buttons labelled
//! from `mes/MainMenu.mes`. Both menus share the panel and styling, as in the original.
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
use crate::fonts::{ArcanumFonts, ORIGINAL_SCALE, UiFont};
use crate::loading::LoadRequest;
use crate::mes_loader::MesAsset;
use crate::resource_pack::paths::{backgrounds, text};
use crate::settings::BASELINE_HEIGHT;
use crate::widgets::{HoverText, WidgetsPlugin};

/// Which screen is showing.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    /// The Single Player menu (New Game, Load Game, ...).
    SinglePlayer,
    /// The Load Game screen (see [`crate::load_game`]).
    LoadGame,
    /// The loading screen on the way into the game (see [`crate::loading`]).
    Loading,
    /// Placeholder for the game world.
    InGame,
    Options,
    Credits,
}

/// What a menu button does.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    // Main menu
    SinglePlayer,
    Options,
    Credits,
    ExitGame,
    // Single Player menu
    NewGame,
    LoadGame,
    /// Resumes the most recent save. Replaces the original's Last Save.
    Continue,
    /// Back to the main menu.
    Cancel,
}

impl MenuAction {
    /// False for entries whose feature does not exist yet: shown dimmed, not clickable.
    pub fn is_available(self) -> bool {
        !matches!(self, Self::Continue)
    }
}

/// Main menu entries, top to bottom, with their message ids in `MainMenu.mes`. The original's
/// MultiPlayer (11) is left out.
pub const MENU_ENTRIES: [(u32, MenuAction); 4] = [
    (10, MenuAction::SinglePlayer),
    (12, MenuAction::Options),
    (13, MenuAction::Credits),
    (14, MenuAction::ExitGame),
];

/// Message ids of [`MENU_ENTRIES`], in order.
pub const MENU_ENTRY_IDS: [u32; 4] = [10, 12, 13, 14];

/// Single Player menu entries, top to bottom, with their message ids in `MainMenu.mes`. The
/// original's View Intro (53) is left out, and Continue (34, from the in-game menu) takes the
/// place of Last Save (52).
pub const SINGLE_PLAYER_ENTRIES: [(u32, MenuAction); 4] = [
    (50, MenuAction::NewGame),
    (51, MenuAction::LoadGame),
    (34, MenuAction::Continue),
    (54, MenuAction::Cancel),
];

/// The menu shown in `state`, if it is a menu.
pub fn menu_entries(state: AppState) -> Option<&'static [(u32, MenuAction)]> {
    match state {
        AppState::MainMenu => Some(&MENU_ENTRIES),
        AppState::SinglePlayer => Some(&SINGLE_PLAYER_ENTRIES),
        _ => None,
    }
}

/// Menu text in the original's dark red, and the much brighter red of the hovered entry.
/// Measured from the solid strokes in screenshots of the original Single Player menu
/// ("Load Game" unhovered, "New Game" hovered).
pub const TEXT_COLOR: Color = Color::srgb_u8(96, 28, 20);
pub const HOVER_COLOR: Color = Color::srgb_u8(184, 48, 33);
/// Entries that cannot be used yet.
pub const DISABLED_COLOR: Color = Color::srgb_u8(58, 42, 36);

// Layout, measured on the original 800x600 art (pixels from its top-left corner), then scaled to
// the 1080 baseline. Replacement art with a different layout needs new numbers here.
const ART_SCALE: f32 = ORIGINAL_SCALE;
/// Horizontal centre of the framed panel, relative to the middle of the art.
pub(crate) const PANEL_CENTER_OFFSET_X: f32 = 12.0 * ART_SCALE;
pub(crate) const PANEL_WIDTH: f32 = 400.0 * ART_SCALE;
const ROW_HEIGHT: f32 = 55.0 * ART_SCALE;
/// Top of the first row.
pub(crate) const PANEL_TOP: f32 = (145.0 - 55.0 / 2.0) * ART_SCALE;

/// A menu entry's text, tagged with its message id. The text entity is also the button.
#[derive(Component)]
pub struct MenuLabel(pub u32);

/// Root of a menu's UI (main or Single Player); despawned when leaving its state.
#[derive(Component)]
pub struct OnMainMenu;

/// Marks the background sprite until its image has loaded and it has been fitted to the
/// baseline height.
#[derive(Component)]
pub struct FitToBaseline;

/// `MainMenu.mes`, kept loaded for every menu screen.
#[derive(Resource)]
pub struct MenuText(pub Handle<MesAsset>);

/// Present while a menu's buttons still need spawning (waiting for `MainMenu.mes`).
#[derive(Resource)]
struct PendingMenu;

fn request_menu(mut commands: Commands) {
    commands.insert_resource(PendingMenu);
}

pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup_menu)
            .add_systems(OnEnter(AppState::MainMenu), request_menu)
            .add_systems(OnEnter(AppState::SinglePlayer), request_menu)
            .add_systems(Update, (fit_background, spawn_menu.run_if(resource_exists::<PendingMenu>)))
            .add_observer(on_activate);
        if !app.is_plugin_added::<WidgetsPlugin>() {
            app.add_plugins(WidgetsPlugin);
        }
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

/// Spawns the current menu's button column once `MainMenu.mes` has loaded.
fn spawn_menu(
    mut commands: Commands,
    state: Res<State<AppState>>,
    menu_text: Res<MenuText>,
    mes: Res<Assets<MesAsset>>,
    assets: Res<AssetServer>,
    fonts: Option<Res<ArcanumFonts>>,
) {
    let state = *state.get();
    let Some(entries) = menu_entries(state) else {
        commands.remove_resource::<PendingMenu>();
        return;
    };
    let Some(mes) = mes.get(&menu_text.0) else {
        if let Some(bevy::asset::LoadState::Failed(err)) = assets.get_load_state(&menu_text.0) {
            error!("cannot load {}: {err}", text::MAIN_MENU);
            commands.remove_resource::<PendingMenu>();
        }
        return;
    };
    let fonts = fonts.map(|f| f.clone()).unwrap_or_default();

    commands
        .spawn((OnMainMenu, DespawnOnExit(state), panel_node()))
        .with_children(|column| {
            for &(id, action) in entries {
                let Some(label) = mes.get(id) else {
                    warn!("{} has no message {id}", text::MAIN_MENU);
                    continue;
                };
                // The row only spaces the entries out. The button is the text itself, so hover and
                // clicks only register over the words, not across the whole panel width.
                column
                    .spawn(Node {
                        height: Val::Px(ROW_HEIGHT),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        let mut text = row.spawn((MenuLabel(id), action));
                        if action.is_available() {
                            text.insert((
                                Button,
                                ActivateOnPress,
                                Hovered::default(),
                                HoverText { normal: TEXT_COLOR, hover: HOVER_COLOR },
                                TextColor(TEXT_COLOR),
                            ));
                        } else {
                            text.insert(TextColor(DISABLED_COLOR));
                        }
                        fonts.insert_text(&mut text, label, UiFont::Large);
                    });
            }
        });
    commands.remove_resource::<PendingMenu>();
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
    let Ok(&action) = actions.get(activate.entity) else { return };
    if !action.is_available() {
        return;
    }
    if let Some(sounds) = &sounds {
        sounds.play_click(&mut commands);
    }
    match action {
        MenuAction::SinglePlayer => next_state.set(AppState::SinglePlayer),
        MenuAction::Options => next_state.set(AppState::Options),
        MenuAction::Credits => next_state.set(AppState::Credits),
        MenuAction::ExitGame => {
            exit.write(AppExit::Success);
        }
        MenuAction::NewGame => {
            // Character selection and creation will come first; for now go straight to loading.
            commands.insert_resource(LoadRequest::new(AppState::InGame));
            next_state.set(AppState::Loading);
        }
        MenuAction::Cancel => next_state.set(AppState::MainMenu),
        MenuAction::LoadGame => next_state.set(AppState::LoadGame),
        MenuAction::Continue => {}
    }
}
