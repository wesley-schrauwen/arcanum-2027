//! Placeholder screens for the menu destinations that are not built yet. Each shows its title
//! (from `MainMenu.mes`) on the menu panel; Escape returns to the main menu.

use bevy::prelude::*;

use crate::fonts::{ArcanumFonts, UiFont};
use crate::main_menu::{AppState, HOVER_COLOR, MenuText, TEXT_COLOR, panel_node};
use crate::mes_loader::MesAsset;

/// Marks the root of a placeholder screen.
#[derive(Component)]
pub struct PlaceholderScreen(pub AppState);

/// Placeholder screens with the `MainMenu.mes` id of their title.
const PLACEHOLDERS: [(AppState, u32); 3] =
    [(AppState::SinglePlayer, 10), (AppState::Options, 12), (AppState::Credits, 13)];

/// Shown under the title. Not from the game's text, so English only for now.
pub const RETURN_HINT: &str = "Press Esc to return";

pub struct ScreensPlugin;

impl Plugin for ScreensPlugin {
    fn build(&self, app: &mut App) {
        for (state, title_id) in PLACEHOLDERS {
            app.add_systems(OnEnter(state), move |commands: Commands, text: Option<Res<MenuText>>, mes: Res<Assets<MesAsset>>, fonts: Option<Res<ArcanumFonts>>| {
                spawn_placeholder(commands, state, title_id, text, mes, fonts)
            });
        }
        app.add_systems(Update, return_to_menu.run_if(not(in_state(AppState::MainMenu))));
    }
}

fn spawn_placeholder(
    mut commands: Commands,
    state: AppState,
    title_id: u32,
    text: Option<Res<MenuText>>,
    mes: Res<Assets<MesAsset>>,
    fonts: Option<Res<ArcanumFonts>>,
) {
    let title = text
        .and_then(|t| mes.get(&t.0))
        .and_then(|m| m.get(title_id))
        .map(str::to_string)
        .unwrap_or_else(|| format!("{state:?}"));
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

fn return_to_menu(keys: Option<Res<ButtonInput<KeyCode>>>, mut next_state: ResMut<NextState<AppState>>) {
    if keys.is_some_and(|k| k.just_pressed(KeyCode::Escape)) {
        next_state.set(AppState::MainMenu);
    }
}
