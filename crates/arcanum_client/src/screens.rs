//! Placeholder screens for the menu destinations that are not built yet (Options has its own
//! screen in [`crate::options`]). Each shows its title (from `MainMenu.mes`) on the menu panel.
//!
//! Escape goes back to the main menu, except on the loading screen, which cannot be left.

use bevy::prelude::*;

use crate::fonts::{ArcanumFonts, UiFont};
use crate::main_menu::{AppState, HOVER_COLOR, MenuText, TEXT_COLOR, panel_node};
use crate::mes_loader::MesAsset;

/// Marks the root of a placeholder screen.
#[derive(Component)]
pub struct PlaceholderScreen(pub AppState);

/// Placeholder screens with the `MainMenu.mes` id of their title (if the game has one) and an
/// English fallback.
const PLACEHOLDERS: [(AppState, Option<u32>, &str); 2] =
    [(AppState::InGame, None, "The world goes here"), (AppState::Credits, Some(13), "Credits")];

/// Shown under the title. Not from the game's text, so English only for now.
pub const RETURN_HINT: &str = "Press Esc to return";

pub struct ScreensPlugin;

impl Plugin for ScreensPlugin {
    fn build(&self, app: &mut App) {
        for (state, title_id, fallback) in PLACEHOLDERS {
            app.add_systems(OnEnter(state), move |commands: Commands, text: Option<Res<MenuText>>, mes: Res<Assets<MesAsset>>, fonts: Option<Res<ArcanumFonts>>| {
                spawn_placeholder(commands, state, title_id, fallback, text, mes, fonts)
            });
        }
        app.add_systems(
            Update,
            // Options handles Escape itself (it may need to revert a resolution change first).
            return_to_menu
                .run_if(not(in_state(AppState::MainMenu)))
                .run_if(not(in_state(AppState::Options)))
                .run_if(not(in_state(AppState::Loading))),
        );
    }
}

fn spawn_placeholder(
    mut commands: Commands,
    state: AppState,
    title_id: Option<u32>,
    fallback: &str,
    text: Option<Res<MenuText>>,
    mes: Res<Assets<MesAsset>>,
    fonts: Option<Res<ArcanumFonts>>,
) {
    let title = title_id
        .and_then(|id| text.and_then(|t| mes.get(&t.0)).and_then(|m| m.get(id)))
        .unwrap_or(fallback)
        .to_string();
    let fonts = fonts.map(|f| f.clone()).unwrap_or_default();

    commands
        .spawn((PlaceholderScreen(state), DespawnOnExit(state), Node { align_items: AlignItems::Center, row_gap: Val::Px(UiFont::Large.ttf_size()), ..panel_node() }))
        .with_children(|panel| {
            let mut heading = panel.spawn(TextColor(HOVER_COLOR));
            fonts.insert_text(&mut heading, &title, UiFont::Large);
            let mut hint = panel.spawn(TextColor(TEXT_COLOR));
            fonts.insert_text(&mut hint, RETURN_HINT, UiFont::Small);
        });
}

/// The screen Escape goes back to from `state`.
pub fn back_from(_state: AppState) -> AppState {
    AppState::MainMenu
}

fn return_to_menu(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    state: Res<State<AppState>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keys.is_some_and(|k| k.just_pressed(KeyCode::Escape)) {
        next_state.set(back_from(*state.get()));
    }
}
