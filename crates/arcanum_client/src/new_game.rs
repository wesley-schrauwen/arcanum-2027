//! The New Game screen: the original's character picker (`PreGenCharacterBase`, 800x400) in the
//! shared menu frame ([`crate::menu_window`]).
//!
//! Scaffold only: the portrait box, the arrows beside it, the four info bars and the
//! description panels are art, with nothing in them yet. Back returns to the Single Player menu;
//! Next (hover help "Next Screen", `MainMenu.mes` 7003) goes through the loading screen into
//! the world placeholder.

use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use crate::audio::MenuSounds;
use crate::fonts::ArcanumFonts;
use crate::loading::LoadRequest;
use crate::main_menu::AppState;
use crate::menu_window::{HoverHelp, MenuWindowPlugin, WindowArt, screen_root, spawn_frame};
use crate::resource_pack::paths::interface;

/// What a New Game button does.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewGameAction {
    Back,
    Next,
}

/// Root of the New Game screen.
#[derive(Component)]
pub struct NewGameScreen;

#[derive(Resource)]
struct NewGameArt {
    background: Handle<Image>,
}

pub struct NewGamePlugin;

impl Plugin for NewGamePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<MenuWindowPlugin>() {
            app.add_plugins(MenuWindowPlugin);
        }
        app.add_systems(Startup, |mut commands: Commands, assets: Res<AssetServer>| {
            commands.insert_resource(NewGameArt { background: assets.load(interface::PREGEN_BACK) });
        })
        .add_systems(OnEnter(AppState::NewGame), build_screen)
        .add_observer(on_activate);
    }
}

fn build_screen(mut commands: Commands, art: Res<NewGameArt>, frame: Res<WindowArt>, fonts: Option<Res<ArcanumFonts>>) {
    let fonts = fonts.map(|f| f.clone()).unwrap_or_default();
    commands.spawn((NewGameScreen, DespawnOnExit(AppState::NewGame), screen_root())).with_children(|screen| {
        spawn_frame(
            screen,
            &frame,
            &fonts,
            art.background.clone(),
            NewGameAction::Back,
            (NewGameAction::Next, HoverHelp(7003, "Next Screen")),
        );
    });
}

fn on_activate(
    activate: On<Activate>,
    actions: Query<&NewGameAction>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    sounds: Option<Res<MenuSounds>>,
) {
    let Ok(&action) = actions.get(activate.entity) else { return };
    if let Some(sounds) = &sounds {
        sounds.play_button_press(&mut commands);
    }
    match action {
        NewGameAction::Back => next_state.set(AppState::SinglePlayer),
        NewGameAction::Next => {
            // Character choice comes later; for now straight into the world placeholder.
            commands.insert_resource(LoadRequest::new(AppState::InGame));
            next_state.set(AppState::Loading);
        }
    }
}
