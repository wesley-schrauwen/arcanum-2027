//! The Load Game screen, reached from the Single Player menu.
//!
//! Built from the original's art and its screen table in Arcanum.exe (0x5c4560): the 800x41 top
//! bar, the 800x400 save/load background and the 800x159 bottom bar, stacked into the 800x600
//! screen. The background has a save list on the left, a portrait socket above it, and an oval
//! window for the save's screenshot. Buttons, at the original's positions:
//! - Back (`BackBut`) and Load (`NextBut`) in the bottom bar's round sockets;
//! - Delete (`cncl_big`) in the small circle under the list;
//! - the info toggle (`TextToggle`) at the top of the oval.
//!
//! Hovering a button shows its hover help (`MainMenu.mes` 7002, 7020, 7022, 7025) in the
//! bottom bar's text box.
//!
//! Saving does not exist yet, so [`SaveGames`] is empty unless something fills it. The list,
//! selection, Delete (which only removes the entry from [`SaveGames`]) and the info toggle work
//! on whatever it holds; Load takes the selected save through the loading screen into the world
//! placeholder.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, ActivateOnPress, Button};

use crate::art_loader::ArtFrames;
use crate::audio::MenuSounds;
use crate::fonts::{ArcanumFonts, ORIGINAL_SCALE, UiFont};
use crate::loading::LoadRequest;
use crate::main_menu::{AppState, HOVER_COLOR, MenuText};
use crate::mes_loader::MesAsset;
use crate::resource_pack::paths::interface;
use crate::widgets::{FrameButton, HoverText, WidgetsPlugin};

/// A save game as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSummary {
    /// The name the player gave the save.
    pub name: String,
    pub level: u32,
    pub location: String,
    /// When it was saved, ready to display.
    pub saved_at: String,
}

/// The saves the Load Game screen lists, newest first. Empty until saving exists.
#[derive(Resource, Debug, Clone, Default)]
pub struct SaveGames(pub Vec<SaveSummary>);

/// Selection and the info toggle. Reset each time the screen opens.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoadGameState {
    pub selected: Option<usize>,
    /// Shows the selected save's details over its screenshot.
    pub show_info: bool,
}

/// What a Load Game button does.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadGameAction {
    Load,
    Back,
    Delete,
    ToggleInfo,
    Select(usize),
}

/// Root of the Load Game screen (rebuilt when the list or selection changes).
#[derive(Component)]
pub struct LoadGameScreen;

/// A row in the save list.
#[derive(Component, Debug)]
pub struct SaveRow(pub usize);

/// The details shown over the screenshot when the info toggle is on.
#[derive(Component)]
pub struct SaveInfo;

/// The bottom bar's hover help text.
#[derive(Component)]
pub struct HoverHelpText;

/// `MainMenu.mes` id of a button's hover help, and its English fallback.
#[derive(Component, Debug, Clone, Copy)]
pub struct HoverHelp(pub u32, pub &'static str);

/// Shown in the list when there are no saves. Not in the game's text.
pub const NO_SAVES: &str = "No saved games";

/// Save list entries.
pub const ROW_COLOR: Color = Color::srgb_u8(214, 186, 128);
/// Details over the screenshot and the hover help.
pub const INFO_COLOR: Color = Color::srgb_u8(236, 220, 180);

// Layout in original 800x600 pixels; everything is multiplied by ORIGINAL_SCALE.
const TOP_BAR_HEIGHT: f32 = 41.0;
const BACKGROUND_HEIGHT: f32 = 400.0;
const BOTTOM_BAR_TOP: f32 = TOP_BAR_HEIGHT + BACKGROUND_HEIGHT;
/// Inside the list box (background 33,111 - 209,343), offset to the screen.
const LIST: Rect = Rect { min: Vec2::new(38.0, 116.0 + TOP_BAR_HEIGHT), max: Vec2::new(204.0, 338.0 + TOP_BAR_HEIGHT) };
/// Portrait ring over the socket above the list.
const LENS_AT: Vec2 = Vec2::new(84.0, 8.0 + TOP_BAR_HEIGHT);
const LENS_SIZE: Vec2 = Vec2::new(89.0, 89.0);
/// Screenshot frame over the oval window.
const SHOT: Rect = Rect { min: Vec2::new(281.0, 55.0 + TOP_BAR_HEIGHT), max: Vec2::new(749.0, 355.0 + TOP_BAR_HEIGHT) };
/// Buttons: positions from the original's screen table.
const BACK_AT: Vec2 = Vec2::new(69.0, BOTTOM_BAR_TOP + 55.0);
const LOAD_AT: Vec2 = Vec2::new(675.0, BOTTOM_BAR_TOP + 55.0);
const ROUND_BUTTON: Vec2 = Vec2::new(61.0, 61.0);
const DELETE_AT: Vec2 = Vec2::new(115.0, 396.0);
const DELETE_SIZE: Vec2 = Vec2::new(38.0, 37.0);
const TOGGLE_AT: Vec2 = Vec2::new(499.0, 82.0);
const TOGGLE_SIZE: Vec2 = Vec2::new(33.0, 33.0);
/// The bottom bar's text box.
const HELP_BOX: Rect = Rect { min: Vec2::new(206.0, BOTTOM_BAR_TOP + 58.0), max: Vec2::new(594.0, BOTTOM_BAR_TOP + 150.0) };

/// Art handles, loaded at startup so the screen opens instantly.
#[derive(Resource)]
struct LoadGameArt {
    top_bar: Handle<Image>,
    bottom_bar: Handle<Image>,
    background: Handle<Image>,
    lens: Handle<Image>,
    shot_cover: Handle<Image>,
    next: Handle<ArtFrames>,
    back: Handle<ArtFrames>,
    delete: Handle<ArtFrames>,
    toggle: Handle<ArtFrames>,
}

/// Present when the screen needs (re)building.
#[derive(Resource)]
struct RebuildLoadGame;

pub struct LoadGamePlugin;

impl Plugin for LoadGamePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<WidgetsPlugin>() {
            app.add_plugins(WidgetsPlugin);
        }
        app.init_resource::<SaveGames>()
            .init_resource::<LoadGameState>()
            .add_systems(Startup, load_art)
            .add_systems(OnEnter(AppState::LoadGame), enter)
            .add_systems(OnExit(AppState::LoadGame), |mut commands: Commands| {
                commands.remove_resource::<RebuildLoadGame>();
            })
            .add_systems(
                Update,
                (
                    request_rebuild,
                    build_screen.run_if(resource_exists::<RebuildLoadGame>),
                    show_hover_help,
                    show_toggle_frame,
                )
                    .chain()
                    .run_if(in_state(AppState::LoadGame)),
            )
            .add_observer(on_activate);
    }
}

fn load_art(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(LoadGameArt {
        top_bar: assets.load(interface::TOP_BAR),
        bottom_bar: assets.load(interface::BOTTOM_BAR),
        background: assets.load(interface::SAVE_LOAD_BACK),
        lens: assets.load(interface::SAVE_LOAD_LENS),
        shot_cover: assets.load(interface::SAVE_SHOT_COVER),
        next: assets.load(interface::NEXT_BUTTON),
        back: assets.load(interface::BACK_BUTTON),
        delete: assets.load(interface::DELETE_BUTTON),
        toggle: assets.load(interface::TEXT_TOGGLE),
    });
}

fn enter(mut commands: Commands, saves: Res<SaveGames>, mut state: ResMut<LoadGameState>) {
    // The newest save starts selected, as there is nothing else to show otherwise.
    *state = LoadGameState { selected: (!saves.0.is_empty()).then_some(0), show_info: false };
    commands.insert_resource(RebuildLoadGame);
}

/// Rebuilds when the saves or the selection change.
fn request_rebuild(mut commands: Commands, saves: Res<SaveGames>, state: Res<LoadGameState>) {
    if saves.is_changed() || state.is_changed() {
        commands.insert_resource(RebuildLoadGame);
    }
}

fn px(v: f32) -> Val {
    Val::Px(v * ORIGINAL_SCALE)
}

/// An absolutely positioned node at `rect` (original pixels).
fn at(rect: Rect) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(rect.min.x),
        top: px(rect.min.y),
        width: px(rect.width()),
        height: px(rect.height()),
        ..default()
    }
}

fn rect(at: Vec2, size: Vec2) -> Rect {
    Rect::from_corners(at, at + size)
}

fn image_button(action: LoadGameAction, help: HoverHelp, frames: &Handle<ArtFrames>, rect: Rect) -> impl Bundle {
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

#[allow(clippy::too_many_arguments)]
fn build_screen(
    mut commands: Commands,
    existing: Query<Entity, With<LoadGameScreen>>,
    art: Res<LoadGameArt>,
    saves: Res<SaveGames>,
    state: Res<LoadGameState>,
    fonts: Option<Res<ArcanumFonts>>,
) {
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<RebuildLoadGame>();
    let fonts = fonts.map(|f| f.clone()).unwrap_or_default();
    let selected = state.selected.and_then(|i| saves.0.get(i).map(|s| (i, s)));

    commands
        .spawn((
            LoadGameScreen,
            DespawnOnExit(AppState::LoadGame),
            // Black behind the art's transparent windows (the menu background stays loaded).
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
        ))
        .with_children(|screen| {
            screen.spawn((ImageNode::new(art.top_bar.clone()), at(rect(Vec2::ZERO, Vec2::new(800.0, TOP_BAR_HEIGHT)))));
            screen.spawn((
                ImageNode::new(art.background.clone()),
                at(rect(Vec2::new(0.0, TOP_BAR_HEIGHT), Vec2::new(800.0, BACKGROUND_HEIGHT))),
            ));
            screen.spawn((
                ImageNode::new(art.bottom_bar.clone()),
                at(rect(Vec2::new(0.0, BOTTOM_BAR_TOP), Vec2::new(800.0, 159.0))),
            ));
            // The portrait would go under the ring; saves have no portrait yet.
            screen.spawn((ImageNode::new(art.lens.clone()), at(rect(LENS_AT, LENS_SIZE))));
            // The screenshot would go under the frame; saves have no screenshot yet.
            screen.spawn((ImageNode::new(art.shot_cover.clone()), at(SHOT)));

            // Save list.
            screen
                .spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2.0), overflow: Overflow::clip(), ..at(LIST) })
                .with_children(|list| {
                    if saves.0.is_empty() {
                        let mut empty = list.spawn(TextColor(ROW_COLOR));
                        fonts.insert_text(&mut empty, NO_SAVES, UiFont::Small);
                    }
                    for (i, save) in saves.0.iter().enumerate() {
                        let is_selected = Some(i) == state.selected;
                        let colour = if is_selected { HOVER_COLOR } else { ROW_COLOR };
                        let mut row = list.spawn((
                            SaveRow(i),
                            LoadGameAction::Select(i),
                            Button,
                            ActivateOnPress,
                            Hovered::default(),
                            HoverText { normal: colour, hover: HOVER_COLOR },
                            TextColor(colour),
                        ));
                        fonts.insert_text(&mut row, &save.name, UiFont::Small);
                    }
                });

            // Details over the screenshot.
            if state.show_info
                && let Some((_, save)) = selected
            {
                screen
                    .spawn((
                        SaveInfo,
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            row_gap: px(4.0),
                            ..at(SHOT)
                        },
                    ))
                    .with_children(|info| {
                        for line in [save.name.clone(), format!("Level {}", save.level), save.location.clone(), save.saved_at.clone()] {
                            let mut text = info.spawn(TextColor(INFO_COLOR));
                            fonts.insert_text(&mut text, &line, UiFont::Small);
                        }
                    });
            }

            screen.spawn(image_button(LoadGameAction::Back, HoverHelp(7002, "Previous Screen"), &art.back, rect(BACK_AT, ROUND_BUTTON)));
            screen.spawn(image_button(LoadGameAction::Load, HoverHelp(7020, "Load Game"), &art.next, rect(LOAD_AT, ROUND_BUTTON)));
            screen.spawn(image_button(LoadGameAction::Delete, HoverHelp(7022, "Delete Save Game"), &art.delete, rect(DELETE_AT, DELETE_SIZE)));
            screen.spawn((
                LoadGameAction::ToggleInfo,
                HoverHelp(7025, "Toggle Info Display"),
                Button,
                ActivateOnPress,
                Hovered::default(),
                ImageNode::default(),
                at(rect(TOGGLE_AT, TOGGLE_SIZE)),
            ));

            screen
                .spawn(Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..at(HELP_BOX) })
                .with_children(|help_box| {
                    let mut help = help_box.spawn((HoverHelpText, TextColor(INFO_COLOR)));
                    fonts.insert_text(&mut help, "", UiFont::Small);
                });
        });
}

/// The info toggle shows frame 1 while on, frame 0 while off.
fn show_toggle_frame(
    art: Res<LoadGameArt>,
    frames: Res<Assets<ArtFrames>>,
    state: Res<LoadGameState>,
    mut toggles: Query<(&LoadGameAction, &mut ImageNode)>,
) {
    let Some(set) = frames.get(&art.toggle) else { return };
    for (action, mut image) in &mut toggles {
        if *action != LoadGameAction::ToggleInfo {
            continue;
        }
        if let Some(frame) = set.frame(state.show_info as usize)
            && image.image != *frame
        {
            image.image = frame.clone();
        }
    }
}

/// Shows the hovered button's help in the bottom bar.
fn show_hover_help(
    buttons: Query<(&HoverHelp, &Hovered)>,
    menu_text: Option<Res<MenuText>>,
    mes: Res<Assets<MesAsset>>,
    mut help: Query<(Option<&mut crate::bitmap_text::BitmapText>, Option<&mut Text>), With<HoverHelpText>>,
) {
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

fn on_activate(
    activate: On<Activate>,
    actions: Query<&LoadGameAction>,
    mut commands: Commands,
    mut saves: ResMut<SaveGames>,
    mut state: ResMut<LoadGameState>,
    mut next_state: ResMut<NextState<AppState>>,
    sounds: Option<Res<MenuSounds>>,
) {
    let Ok(&action) = actions.get(activate.entity) else { return };
    let selected = state.selected.filter(|i| *i < saves.0.len());
    // Load and Delete need a save; without one they do nothing (and stay silent).
    if matches!(action, LoadGameAction::Load | LoadGameAction::Delete) && selected.is_none() {
        return;
    }
    if let Some(sounds) = &sounds {
        match action {
            LoadGameAction::Select(_) => sounds.play_click(&mut commands),
            _ => sounds.play_button_press(&mut commands),
        }
    }
    match action {
        LoadGameAction::Back => next_state.set(AppState::SinglePlayer),
        LoadGameAction::Load => {
            // Reading the save comes with the save system; for now it goes to the world placeholder.
            commands.insert_resource(LoadRequest::new(AppState::InGame));
            next_state.set(AppState::Loading);
        }
        LoadGameAction::Delete => {
            if let Some(i) = selected {
                // A confirmation (`MainMenu.mes` 5150) belongs here once saves are real files.
                saves.0.remove(i);
                state.selected = (!saves.0.is_empty()).then(|| i.min(saves.0.len() - 1));
            }
        }
        LoadGameAction::ToggleInfo => state.show_info = !state.show_info,
        LoadGameAction::Select(i) => {
            if state.selected != Some(i) {
                state.selected = Some(i);
            }
        }
    }
}

/// True once the Load Game screen has been built.
pub fn load_game_ready(screens: Query<(), With<LoadGameScreen>>) -> bool {
    !screens.is_empty()
}

/// Example saves for screenshots (`DEBUG_ARCANUM_SCREEN=loadgame`).
pub fn sample_saves() -> SaveGames {
    let save = |name: &str, level, location: &str, saved_at: &str| SaveSummary {
        name: name.into(),
        level,
        location: location.into(),
        saved_at: saved_at.into(),
    };
    SaveGames(vec![
        save("Before the bridge", 7, "Tarant", "13 Nov 2026 18:40"),
        save("Shrouded Hills", 3, "Shrouded Hills", "12 Nov 2026 21:05"),
        save("Crash site", 1, "Crash site", "12 Nov 2026 20:11"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_original_screen() {
        // The three art pieces stack into 800x600.
        assert_eq!(BOTTOM_BAR_TOP + 159.0, 600.0);
        // Buttons sit inside the screen.
        for (at, size) in [(BACK_AT, ROUND_BUTTON), (LOAD_AT, ROUND_BUTTON), (DELETE_AT, DELETE_SIZE), (TOGGLE_AT, TOGGLE_SIZE)] {
            assert!(at.x + size.x <= 800.0 && at.y + size.y <= 600.0, "{at:?}");
        }
        // The screenshot frame fills the oval window (468x300).
        assert_eq!(SHOT.size(), Vec2::new(468.0, 300.0));
    }
}
