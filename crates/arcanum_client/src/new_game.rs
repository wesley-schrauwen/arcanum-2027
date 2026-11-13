//! The New Game screen: the original's pre-made character picker (`PreGenCharacterBase`,
//! 800x400) in the shared menu frame ([`crate::menu_window`]).
//!
//! Reached from the New Game choice menu (Pick Character). The arrows beside the portrait cycle
//! through the pre-made characters ([`crate::premade`]), wrapping at both ends. As in the original
//! (Arcanum.exe window 0x5c5270, drawn by 0x5458d0 and 0x5448e0; see NOTES.md): the large
//! portrait; bars for the name, gender (`MainMenu.mes` 740+), race (720+) and the background's
//! title; "Initial Stats" (739) atop the upper-right panel; and the background story's body,
//! wrapped, in the large right panel. Bar texts are centred.
//!
//! Back returns to the New Game choice; Next (hover help "Next Screen", `MainMenu.mes` 7003)
//! remembers the chosen character ([`ChosenCharacter`]) and goes through the loading screen
//! into the world placeholder.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, ActivateOnPress, Button};

use crate::art_loader::ArtFrames;
use crate::audio::MenuSounds;
use crate::bitmap_text::Align;
use crate::fonts::{ArcanumFonts, ORIGINAL_SCALE, UiFont};
use crate::loading::LoadRequest;
use crate::main_menu::AppState;
use crate::menu_window::{HoverHelp, INFO_COLOR, MenuWindowPlugin, TOP_BAR_HEIGHT, WindowArt, at, rect, screen_root, spawn_frame};
use crate::premade::{Premade, PremadeCharacters, split_story};
use crate::resource_pack::paths::interface;
use crate::vfs_asset::VfsResource;
use crate::widgets::FrameButton;

/// What a New Game button does.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewGameAction {
    Back,
    Next,
    Previous,
    Following,
}

/// Root of the New Game screen.
#[derive(Component)]
pub struct NewGameScreen;

/// Root of the current character's content (rebuilt when the selection changes).
#[derive(Component)]
pub struct PickerContent;

/// Which pre-made character is showing.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PickerIndex(pub usize);

/// The character picked with Next: its prototype number.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChosenCharacter(pub u32);

/// Marks the text pieces, for tests.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerText {
    Name,
    /// Bars 2-4: gender, race, background title.
    Bar(usize),
    StatsHeading,
    StoryBody,
}

/// "Initial Stats", atop the upper-right panel.
const STATS_HEADING_TEXT: (u32, &str) = (739, "Initial Stats");

#[derive(Resource)]
struct NewGameArt {
    background: Handle<Image>,
    left: Handle<ArtFrames>,
    right: Handle<ArtFrames>,
}

// Layout in original pixels on the background (add TOP_BAR_HEIGHT for the screen).
const PORTRAIT: Rect = Rect { min: Vec2::new(95.0, 28.0), max: Vec2::new(223.0, 156.0) };
const LEFT_ARROW_AT: Vec2 = Vec2::new(43.0, 118.0 - TOP_BAR_HEIGHT);
const RIGHT_ARROW_AT: Vec2 = Vec2::new(244.0, 118.0 - TOP_BAR_HEIGHT);
const ARROW_SIZE: Vec2 = Vec2::new(36.0, 36.0);
const NAME_BAR: Rect = Rect { min: Vec2::new(46.0, 183.0), max: Vec2::new(274.0, 202.0) };
const BARS: [Rect; 3] = [
    Rect { min: Vec2::new(66.0, 243.0), max: Vec2::new(252.0, 262.0) },
    Rect { min: Vec2::new(66.0, 293.0), max: Vec2::new(252.0, 312.0) },
    Rect { min: Vec2::new(66.0, 343.0), max: Vec2::new(252.0, 362.0) },
];
const STATS_HEADING: Rect = Rect { min: Vec2::new(339.0, 23.0), max: Vec2::new(748.0, 42.0) };
const STORY_BODY: Rect = Rect { min: Vec2::new(350.0, 155.0), max: Vec2::new(745.0, 367.0) };

/// On the screen: background pixels moved below the top bar.
fn on_screen(r: Rect) -> Rect {
    Rect::from_corners(r.min + Vec2::Y * TOP_BAR_HEIGHT, r.max + Vec2::Y * TOP_BAR_HEIGHT)
}

/// The three detail bars' text for a character: gender, race, background title.
pub fn bar_texts(p: &Premade) -> [String; 3] {
    [p.gender.clone(), p.race.clone(), split_story(&p.story).0.to_string()]
}

pub struct NewGamePlugin;

impl Plugin for NewGamePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<MenuWindowPlugin>() {
            app.add_plugins(MenuWindowPlugin);
        }
        app.init_resource::<PickerIndex>()
            .add_systems(Startup, |mut commands: Commands, assets: Res<AssetServer>| {
                commands.insert_resource(NewGameArt {
                    background: assets.load(interface::PREGEN_BACK),
                    left: assets.load(interface::PICKER_LEFT),
                    right: assets.load(interface::PICKER_RIGHT),
                });
            })
            .add_systems(OnEnter(AppState::NewGame), (load_premades, build_screen).chain())
            .add_systems(Update, show_character.run_if(in_state(AppState::NewGame)))
            .add_observer(on_activate);
    }
}

/// Reads the pre-made characters the first time the screen opens.
fn load_premades(mut commands: Commands, existing: Option<Res<PremadeCharacters>>, vfs: Option<Res<VfsResource>>, mut index: ResMut<PickerIndex>) {
    index.0 = 0;
    if existing.is_some() {
        return;
    }
    let loaded = vfs
        .and_then(|v| v.0.read().ok().map(|vfs| crate::premade::load(&vfs)))
        .unwrap_or_else(|| Err(crate::premade::LoadError("no game data".into())));
    match loaded {
        Ok(list) => commands.insert_resource(PremadeCharacters(list)),
        Err(e) => {
            warn!("cannot read the pre-made characters: {e}");
            commands.insert_resource(PremadeCharacters::default());
        }
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
        for (action, frames, pos) in [
            (NewGameAction::Previous, &art.left, LEFT_ARROW_AT),
            (NewGameAction::Following, &art.right, RIGHT_ARROW_AT),
        ] {
            screen.spawn((
                action,
                Button,
                ActivateOnPress,
                Hovered::default(),
                FrameButton::arrow(frames.clone()),
                ImageNode::default(),
                at(on_screen(rect(pos, ARROW_SIZE))),
            ));
        }
    });
}

/// (Re)draws the current character when the screen or the selection changes.
fn show_character(
    mut commands: Commands,
    index: Res<PickerIndex>,
    premades: Option<Res<PremadeCharacters>>,
    screens: Query<Entity, Added<NewGameScreen>>,
    contents: Query<Entity, With<PickerContent>>,
    roots: Query<Entity, With<NewGameScreen>>,
    assets: Res<AssetServer>,
    fonts: Option<Res<ArcanumFonts>>,
    menu_text: Option<Res<crate::main_menu::MenuText>>,
    mes: Res<Assets<crate::mes_loader::MesAsset>>,
) {
    let Some(premades) = premades else { return };
    let stats_heading = menu_text
        .as_ref()
        .and_then(|t| mes.get(&t.0))
        .and_then(|m| m.get(STATS_HEADING_TEXT.0))
        .unwrap_or(STATS_HEADING_TEXT.1)
        .to_string();
    if !index.is_changed() && screens.is_empty() && !premades.is_changed() {
        return;
    }
    let Ok(root) = roots.single() else { return };
    for old in &contents {
        commands.entity(old).despawn();
    }
    let Some(p) = premades.0.get(index.0).cloned() else { return };
    let fonts = fonts.map(|f| f.clone()).unwrap_or_default();
    let body = split_story(&p.story).1.to_string();
    let portrait = p.portrait.clone().map(|path| assets.load::<Image>(path));
    let content = commands
        .spawn((PickerContent, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }))
        .with_children(|c| {
            if let Some(image) = portrait {
                c.spawn((ImageNode::new(image), at(on_screen(PORTRAIT))));
            }
            let mut centred = |r: Rect, text: &str, which: PickerText| {
                c.spawn(Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..at(on_screen(r)) })
                    .with_children(|cell| {
                        let mut t = cell.spawn((which, TextColor(INFO_COLOR)));
                        fonts.insert_text(&mut t, text, UiFont::Small);
                    });
            };
            centred(NAME_BAR, &p.name, PickerText::Name);
            for (i, (r, text)) in BARS.iter().zip(bar_texts(&p)).enumerate() {
                centred(*r, &text, PickerText::Bar(i));
            }
            centred(STATS_HEADING, &stats_heading, PickerText::StatsHeading);
            // The body wraps to the panel and shows the whole lines that fit (no scrolling yet).
            c.spawn(Node { overflow: Overflow::clip(), ..at(on_screen(STORY_BODY)) }).with_children(|panel| {
                let mut t = panel.spawn((PickerText::StoryBody, TextColor(INFO_COLOR)));
                let (w, h) = (STORY_BODY.width() * ORIGINAL_SCALE, STORY_BODY.height() * ORIGINAL_SCALE);
                fonts.insert_block(&mut t, &body, UiFont::Small, w, Some(h), Align::Left);
            });
        })
        .id();
    commands.entity(root).add_child(content);
}

fn on_activate(
    activate: On<Activate>,
    actions: Query<&NewGameAction>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    mut index: ResMut<PickerIndex>,
    premades: Option<Res<PremadeCharacters>>,
    sounds: Option<Res<MenuSounds>>,
) {
    let Ok(&action) = actions.get(activate.entity) else { return };
    if let Some(sounds) = &sounds {
        sounds.play_button_press(&mut commands);
    }
    let count = premades.as_ref().map_or(0, |p| p.0.len());
    match action {
        NewGameAction::Back => next_state.set(AppState::NewGameChoice),
        NewGameAction::Previous if count > 0 => index.0 = (index.0 + count - 1) % count,
        NewGameAction::Following if count > 0 => index.0 = (index.0 + 1) % count,
        NewGameAction::Previous | NewGameAction::Following => {}
        NewGameAction::Next => {
            if let Some(p) = premades.as_ref().and_then(|p| p.0.get(index.0)) {
                commands.insert_resource(ChosenCharacter(p.prototype));
            }
            commands.insert_resource(LoadRequest::new(AppState::InGame));
            next_state.set(AppState::Loading);
        }
    }
}
