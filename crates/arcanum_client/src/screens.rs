//! Placeholder screens for the menu destinations that are not built yet. Each shows its title
//! (from `MainMenu.mes`) on the menu panel; Escape returns to the main menu.

use bevy::prelude::*;

use crate::fonts::ArcanumFonts;
use crate::main_menu::{AppState, FONT_SIZE, HOVER_COLOR, MenuText, TEXT_COLOR, panel_node};
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
        .spawn((PlaceholderScreen(state), DespawnOnExit(state), Node { align_items: AlignItems::Center, row_gap: Val::Px(FONT_SIZE), ..panel_node() }))
        .with_children(|panel| {
            panel.spawn((Text::new(title), fonts.serif(FONT_SIZE * 1.2), TextColor(HOVER_COLOR)));
            panel.spawn((Text::new(RETURN_HINT), fonts.serif(FONT_SIZE * 0.6), TextColor(TEXT_COLOR)));
        });
}

fn return_to_menu(keys: Option<Res<ButtonInput<KeyCode>>>, mut next_state: ResMut<NextState<AppState>>) {
    if keys.is_some_and(|k| k.just_pressed(KeyCode::Escape)) {
        next_state.set(AppState::MainMenu);
    }
}
