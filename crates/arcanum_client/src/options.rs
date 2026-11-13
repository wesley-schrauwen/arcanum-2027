//! The Options screen, built from the original's art and text.
//!
//! Layout follows `OptionsMenuBack.ART` (800x600): Game / Video / Sound tabs and Done in the
//! framed box on the left, the tab name on the title plate, and up to eight settings in the
//! 2x4 grid of slots. Each slot is either a slider (`OptionSlider.ART`) or a choice
//! (`OptionMultiChoice.ART`), both stepped with left/right arrow buttons.
//!
//! Changes commit to [`GameSettings`] immediately. The one exception is the resolution: its
//! arrows only pick a pending resolution. Done (or Escape) applies it and shows "Confirm
//! Resolution" before returning to the main menu; Keep keeps it, Revert (or Escape, or
//! [`RESOLUTION_CONFIRM_SECS`] without an answer) puts the previous one back. Either way the
//! screen then returns to the main menu.
//!
//! Labels come from `mes/options_ui.mes` (setting names), `OptionsOffOn`/`OptionsFloats`
//! (values) and `MainMenu.mes` 4000-4003 (tabs and Done). Resolution, Fullscreen and the
//! confirmation dialog are new, so their text is English.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, ActivateOnPress, Button};

use crate::art_loader::ArtFrames;
use crate::audio::MenuSounds;
use crate::bitmap_text::BitmapText;
use crate::fonts::{ArcanumFonts, ORIGINAL_SCALE, UiFont};
use crate::main_menu::{AppState, HOVER_COLOR, MenuText, TEXT_COLOR};
use crate::mes_loader::MesAsset;
use crate::resource_pack::paths;
use crate::settings::{DisplayResolution, Floats, GameSettings};
use crate::widgets::{FrameButton, HoverText, Selected, WidgetsPlugin};

/// Seconds before an unconfirmed resolution change reverts.
pub const RESOLUTION_CONFIRM_SECS: f32 = 15.0;
/// How far one arrow press moves a slider.
pub const SLIDER_STEP: f32 = 0.1;

/// Setting names on the slots.
pub const NAME_COLOR: Color = Color::srgb_u8(214, 186, 128);
/// Values inside the slots.
pub const VALUE_COLOR: Color = Color::srgb_u8(236, 220, 180);
/// The filled part of a slider tube, matching the red marker in the art.
pub const SLIDER_FILL: Color = Color::srgba_u8(176, 40, 28, 220);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum OptionsTab {
    #[default]
    Game,
    Video,
    Sound,
}

impl OptionsTab {
    pub const ALL: [Self; 3] = [Self::Game, Self::Video, Self::Sound];

    /// Settings on this tab, in slot order (left to right, top to bottom).
    pub fn options(self) -> &'static [OptionId] {
        use OptionId::*;
        match self {
            Self::Game => &[AutoAttack, CombatTaunts],
            Self::Video => &[Brightness, TextDuration, Floats, FloatSpeed, Fullscreen, Resolution],
            Self::Sound => &[Effects, Voice, Music],
        }
    }

    /// Message id in `MainMenu.mes`, and the English fallback.
    fn label(self) -> (u32, &'static str) {
        match self {
            Self::Game => (4000, "Game"),
            Self::Video => (4001, "Video"),
            Self::Sound => (4002, "Sound"),
        }
    }
}

/// `MainMenu.mes` id of the Done button.
const DONE_LABEL: (u32, &str) = (4003, "Done");

/// One setting on the Options screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OptionId {
    AutoAttack,
    Brightness,
    TextDuration,
    Floats,
    FloatSpeed,
    CombatTaunts,
    Resolution,
    Fullscreen,
    Effects,
    Voice,
    Music,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionKind {
    Slider,
    Choice,
}

impl OptionId {
    pub fn kind(self) -> OptionKind {
        use OptionId::*;
        match self {
            Brightness | TextDuration | FloatSpeed | Effects | Voice | Music => OptionKind::Slider,
            _ => OptionKind::Choice,
        }
    }

    /// Message id in `options_ui.mes` (if the original had this setting) and English fallback.
    fn name(self) -> (Option<u32>, &'static str) {
        use OptionId::*;
        match self {
            AutoAttack => (Some(104), "Auto Attack"),
            Brightness => (Some(200), "Brightness"),
            TextDuration => (Some(201), "Text Duration"),
            Floats => (Some(202), "Floats"),
            FloatSpeed => (Some(203), "Float Speed"),
            CombatTaunts => (Some(204), "Combat Taunts"),
            Resolution => (None, "Resolution"),
            Fullscreen => (None, "Fullscreen"),
            Effects => (Some(300), "Effects"),
            Voice => (Some(301), "Voice"),
            Music => (Some(302), "Music"),
        }
    }

    fn slider<'a>(self, s: &'a mut GameSettings) -> Option<&'a mut f32> {
        use OptionId::*;
        Some(match self {
            Brightness => &mut s.brightness,
            TextDuration => &mut s.text_duration,
            FloatSpeed => &mut s.float_speed,
            Effects => &mut s.effects_volume,
            Voice => &mut s.voice_volume,
            Music => &mut s.music_volume,
            _ => return None,
        })
    }

    fn toggle<'a>(self, s: &'a mut GameSettings) -> Option<&'a mut bool> {
        use OptionId::*;
        Some(match self {
            AutoAttack => &mut s.auto_attack,
            CombatTaunts => &mut s.combat_taunts,
            Fullscreen => &mut s.fullscreen,
            _ => return None,
        })
    }

    /// A slider's value, `0.0..=1.0`.
    pub fn slider_value(self, s: &GameSettings) -> Option<f32> {
        self.slider(&mut s.clone()).map(|v| *v)
    }

    /// Moves the setting `delta` steps: sliders by [`SLIDER_STEP`] (clamped), choices to the
    /// next or previous value (wrapping).
    pub fn step(self, s: &mut GameSettings, delta: i32) {
        if let Some(v) = self.slider(s) {
            *v = ((*v + delta as f32 * SLIDER_STEP).clamp(0.0, 1.0) * 10.0).round() / 10.0;
            return;
        }
        if let Some(b) = self.toggle(s) {
            if delta % 2 != 0 {
                *b = !*b;
            }
            return;
        }
        let wrap = |index: usize, len: usize| (index as i64 + delta as i64).rem_euclid(len as i64) as usize;
        match self {
            OptionId::Floats => {
                let i = Floats::ALL.iter().position(|f| *f == s.floats).unwrap_or(0);
                s.floats = Floats::ALL[wrap(i, Floats::ALL.len())];
            }
            OptionId::Resolution => s.resolution = step_resolution(s.resolution, delta),
            _ => {}
        }
    }
}

/// The preset `delta` steps from `current`, wrapping. A custom size steps to the nearest preset
/// in that direction.
pub fn step_resolution(current: DisplayResolution, delta: i32) -> DisplayResolution {
    let presets = DisplayResolution::PRESETS;
    let i = match presets.iter().position(|p| *p == current) {
        Some(i) => (i as i64 + delta as i64).rem_euclid(presets.len() as i64) as usize,
        None => {
            let area = |r: &DisplayResolution| r.dimensions().0 as u64 * r.dimensions().1 as u64;
            let size = area(&current);
            if delta > 0 {
                presets.iter().position(|p| area(p) > size).unwrap_or(0)
            } else {
                presets.iter().rposition(|p| area(p) < size).unwrap_or(presets.len() - 1)
            }
        }
    };
    presets[i]
}

/// Resolved labels for the screen; text from the game where it exists, English otherwise.
#[derive(Resource, Debug, Clone)]
pub struct OptionsLabels {
    tabs: [String; 3],
    done: String,
    names: Vec<(OptionId, String)>,
    off_on: [String; 2],
    floats: [String; 3],
}

impl OptionsLabels {
    pub fn tab(&self, tab: OptionsTab) -> &str {
        &self.tabs[OptionsTab::ALL.iter().position(|t| *t == tab).unwrap()]
    }

    pub fn done(&self) -> &str {
        &self.done
    }

    pub fn name(&self, option: OptionId) -> &str {
        self.names.iter().find(|(o, _)| *o == option).map(|(_, n)| n.as_str()).unwrap_or("")
    }

    /// The displayed value of a choice setting; empty for sliders.
    pub fn value(&self, option: OptionId, s: &GameSettings) -> String {
        let on_off = |b: bool| self.off_on[b as usize].clone();
        match option {
            OptionId::AutoAttack => on_off(s.auto_attack),
            OptionId::CombatTaunts => on_off(s.combat_taunts),
            OptionId::Fullscreen => on_off(s.fullscreen),
            OptionId::Floats => self.floats[Floats::ALL.iter().position(|f| *f == s.floats).unwrap()].clone(),
            OptionId::Resolution => {
                let (w, h) = s.resolution.dimensions();
                format!("{w} x {h}")
            }
            _ => String::new(),
        }
    }

    fn resolve(
        main_menu: Option<&MesAsset>,
        ui: Option<&MesAsset>,
        off_on: Option<&MesAsset>,
        floats: Option<&MesAsset>,
    ) -> Self {
        let get = |mes: Option<&MesAsset>, id: u32, fallback: &str| {
            mes.and_then(|m| m.get(id)).unwrap_or(fallback).to_string()
        };
        let all_options = OptionsTab::ALL.iter().flat_map(|t| t.options().iter().copied());
        Self {
            tabs: OptionsTab::ALL.map(|t| get(main_menu, t.label().0, t.label().1)),
            done: get(main_menu, DONE_LABEL.0, DONE_LABEL.1),
            names: all_options
                .map(|o| {
                    let (id, fallback) = o.name();
                    (o, id.map_or(fallback.to_string(), |id| get(ui, id, fallback)))
                })
                .collect(),
            off_on: [get(off_on, 0, "Off"), get(off_on, 1, "On")],
            floats: [get(floats, 0, "Never"), get(floats, 1, "Minimal"), get(floats, 2, "Verbose")],
        }
    }
}

/// Text and art handles, loaded at startup so the screen opens instantly.
#[derive(Resource)]
struct OptionsAssets {
    texts: [Handle<MesAsset>; 3],
    background: Handle<Image>,
    slider: Handle<Image>,
    choice: Handle<Image>,
    arrow_left: Handle<ArtFrames>,
    arrow_right: Handle<ArtFrames>,
}

/// The resolution picked with the arrows, not yet applied. Reset on entering the screen.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingResolution(pub DisplayResolution);

/// The tab being shown.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CurrentTab(pub OptionsTab);

/// Present when the screen needs (re)building: on entry and after a tab change.
#[derive(Resource)]
struct RebuildOptions;

/// A resolution change waiting for the player to keep it.
#[derive(Resource, Debug)]
pub struct ResolutionConfirm {
    pub previous: DisplayResolution,
    pub timer: Timer,
}

/// What an Options screen button does.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionsAction {
    Tab(OptionsTab),
    /// Leaves for the main menu, applying a pending resolution and confirming it first.
    Done,
    Step { option: OptionId, delta: i32 },
    KeepResolution,
    RevertResolution,
}

/// Root of the Options screen (rebuilt on tab change).
#[derive(Component)]
pub struct OptionsScreen;

/// A setting slot.
#[derive(Component, Debug)]
pub struct OptionSlot(pub OptionId);

/// The text showing a choice setting's value.
#[derive(Component, Debug)]
pub struct OptionValue(pub OptionId);

/// The filled part of a slider.
#[derive(Component, Debug)]
pub struct SliderFill(pub OptionId);

/// Root of the resolution confirmation.
#[derive(Component)]
pub struct ResolutionDialog;

/// The countdown line in the resolution confirmation.
#[derive(Component)]
pub struct ResolutionCountdown;

pub struct OptionsPlugin;

impl Plugin for OptionsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<WidgetsPlugin>() {
            app.add_plugins(WidgetsPlugin);
        }
        app.init_resource::<CurrentTab>()
            .add_systems(Startup, load_options_assets)
            .add_systems(OnEnter(AppState::Options), enter_options)
            .add_systems(OnExit(AppState::Options), exit_options)
            .add_systems(
                Update,
                (
                    resolve_labels.run_if(not(resource_exists::<OptionsLabels>)),
                    build_screen.run_if(resource_exists::<RebuildOptions>).run_if(resource_exists::<OptionsLabels>),
                    show_values.run_if(resource_exists::<OptionsLabels>),
                    tick_resolution_confirm.run_if(resource_exists::<ResolutionConfirm>),
                    options_escape,
                )
                    .chain()
                    .run_if(in_state(AppState::Options)),
            )
            .add_observer(on_options_activate);
    }
}

fn load_options_assets(mut commands: Commands, assets: Res<AssetServer>) {
    use paths::{interface, text};
    commands.insert_resource(OptionsAssets {
        texts: [
            assets.load(text::OPTIONS_UI),
            assets.load(text::OPTIONS_OFF_ON),
            assets.load(text::OPTIONS_FLOATS),
        ],
        background: assets.load(interface::OPTIONS_BACK),
        slider: assets.load(interface::OPTION_SLIDER),
        choice: assets.load(interface::OPTION_CHOICE),
        arrow_left: assets.load(interface::ARROW_LEFT),
        arrow_right: assets.load(interface::ARROW_RIGHT),
    });
}

fn enter_options(mut commands: Commands, mut tab: ResMut<CurrentTab>, settings: Res<GameSettings>) {
    tab.0 = OptionsTab::Game;
    commands.insert_resource(PendingResolution(settings.resolution));
    commands.insert_resource(RebuildOptions);
}

fn exit_options(mut commands: Commands) {
    commands.remove_resource::<RebuildOptions>();
    commands.remove_resource::<ResolutionConfirm>();
    commands.remove_resource::<PendingResolution>();
}

/// Builds [`OptionsLabels`] once every text file has loaded or failed.
fn resolve_labels(
    mut commands: Commands,
    assets: Res<AssetServer>,
    options: Res<OptionsAssets>,
    menu_text: Option<Res<MenuText>>,
    mes: Res<Assets<MesAsset>>,
) {
    let settled = |h: &Handle<MesAsset>| {
        mes.get(h).is_some() || matches!(assets.get_load_state(h), Some(bevy::asset::LoadState::Failed(_)))
    };
    let menu = menu_text.as_ref().map(|t| &t.0);
    if !options.texts.iter().all(settled) || !menu.is_none_or(settled) {
        return;
    }
    let [ui, off_on, floats] = &options.texts;
    commands.insert_resource(OptionsLabels::resolve(
        menu.and_then(|h| mes.get(h)),
        mes.get(ui),
        mes.get(off_on),
        mes.get(floats),
    ));
}

// Layout in original 800x600 pixels; everything is multiplied by ORIGINAL_SCALE.
const S: f32 = ORIGINAL_SCALE;
/// Tab box (inside the framed panel on the left).
const TAB_BOX: Rect = Rect { min: Vec2::new(48.0, 205.0), max: Vec2::new(214.0, 392.0) };
/// Title plate.
const TITLE: Rect = Rect { min: Vec2::new(360.0, 58.0), max: Vec2::new(622.0, 104.0) };
const SLOT_COLUMNS: [f32; 2] = [261.0, 531.0];
const SLOT_ROWS: [f32; 4] = [122.0, 196.0, 271.0, 346.0];
const SLOT_SIZE: Vec2 = Vec2::new(213.0, 56.0);
/// Inside a slot: the name strip, the arrow sockets and the tube/value window.
const SLOT_NAME: Rect = Rect { min: Vec2::new(0.0, 1.0), max: Vec2::new(213.0, 21.0) };
const ARROW_LEFT_AT: Vec2 = Vec2::new(6.0, 20.0);
const ARROW_RIGHT_AT: Vec2 = Vec2::new(178.0, 20.0);
const ARROW_SIZE: Vec2 = Vec2::new(29.0, 30.0);
const WINDOW: Rect = Rect { min: Vec2::new(45.0, 25.0), max: Vec2::new(171.0, 49.0) };
const TUBE: Rect = Rect { min: Vec2::new(46.0, 28.0), max: Vec2::new(170.0, 46.0) };

fn px(v: f32) -> Val {
    Val::Px(v * S)
}

/// An absolutely positioned node at `rect` (original pixels).
fn at(rect: Rect) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(rect.min.x),
        top: px(rect.min.y),
        width: px(rect.width()),
        height: px(rect.height()),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn text_button(colours: HoverText, action: OptionsAction) -> impl Bundle {
    (action, Button, ActivateOnPress, Hovered::default(), colours, TextColor(colours.normal))
}

#[allow(clippy::too_many_arguments)]
fn build_screen(
    mut commands: Commands,
    existing: Query<Entity, With<OptionsScreen>>,
    tab: Res<CurrentTab>,
    labels: Res<OptionsLabels>,
    art: Res<OptionsAssets>,
    fonts: Option<Res<ArcanumFonts>>,
) {
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<RebuildOptions>();
    let fonts = fonts.map(|f| f.clone()).unwrap_or_default();
    let red = HoverText { normal: TEXT_COLOR, hover: HOVER_COLOR };

    commands
        .spawn((
            OptionsScreen,
            DespawnOnExit(AppState::Options),
            ImageNode::new(art.background.clone()),
            // The 800x600 screen, centred and filling the 1080 baseline height like the menu art.
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
            // Text sizes its own node to the words, so each piece sits in a box that centres it.
            screen.spawn(at(TITLE)).with_children(|plate| {
                let mut title = plate.spawn(TextColor(HOVER_COLOR));
                fonts.insert_text(&mut title, labels.tab(tab.0), UiFont::Large);
            });

            screen
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceEvenly,
                    ..at(TAB_BOX)
                })
                .with_children(|tabs| {
                    for t in OptionsTab::ALL {
                        let mut button = tabs.spawn(text_button(red, OptionsAction::Tab(t)));
                        if t == tab.0 {
                            button.insert(Selected);
                        }
                        fonts.insert_text(&mut button, labels.tab(t), UiFont::Large);
                    }
                    let mut done = tabs.spawn(text_button(red, OptionsAction::Done));
                    fonts.insert_text(&mut done, labels.done(), UiFont::Large);
                });

            let slot_rect = |index: usize| {
                let corner = Vec2::new(SLOT_COLUMNS[index % 2], SLOT_ROWS[index / 2]);
                Rect::from_corners(corner, corner + SLOT_SIZE)
            };
            for (index, &option) in tab.0.options().iter().enumerate() {
                let slot = slot_rect(index);
                let slot_art = match option.kind() {
                    OptionKind::Slider => art.slider.clone(),
                    OptionKind::Choice => art.choice.clone(),
                };
                screen
                    .spawn((OptionSlot(option), ImageNode::new(slot_art), Node { justify_content: JustifyContent::Default, align_items: AlignItems::Default, ..at(slot) }))
                    .with_children(|slot| {
                        slot.spawn(at(SLOT_NAME)).with_children(|strip| {
                            let mut name = strip.spawn(TextColor(NAME_COLOR));
                            fonts.insert_text(&mut name, labels.name(option), UiFont::Small);
                        });

                        match option.kind() {
                            OptionKind::Slider => {
                                slot.spawn((
                                    SliderFill(option),
                                    BackgroundColor(SLIDER_FILL),
                                    Node { width: Val::Px(0.0), ..at(TUBE) },
                                ));
                            }
                            OptionKind::Choice => {
                                slot.spawn(at(WINDOW)).with_children(|window| {
                                    let mut value = window.spawn((OptionValue(option), TextColor(VALUE_COLOR)));
                                    fonts.insert_text(&mut value, &labels.value(option, &GameSettings::default()), UiFont::Small);
                                });
                            }
                        }

                        for (frames, position, delta) in
                            [(&art.arrow_left, ARROW_LEFT_AT, -1), (&art.arrow_right, ARROW_RIGHT_AT, 1)]
                        {
                            slot.spawn((
                                OptionsAction::Step { option, delta },
                                Button,
                                ActivateOnPress,
                                Hovered::default(),
                                FrameButton::arrow(frames.clone()),
                                ImageNode::default(),
                                at(Rect::from_corners(position, position + ARROW_SIZE)),
                            ));
                        }
                    });
            }
        });
}

/// Keeps value texts and slider fills in step with [`GameSettings`] (and the pending
/// resolution).
fn show_values(
    settings: Res<GameSettings>,
    pending: Option<Res<PendingResolution>>,
    labels: Res<OptionsLabels>,
    mut values: Query<(&OptionValue, Option<&mut BitmapText>, Option<&mut Text>)>,
    mut fills: Query<(&SliderFill, &mut Node)>,
) {
    let mut shown = settings.clone();
    if let Some(pending) = &pending {
        shown.resolution = pending.0;
    }
    for (value, bitmap, text) in &mut values {
        let wanted = labels.value(value.0, &shown);
        if let Some(mut bitmap) = bitmap {
            if bitmap.text != wanted {
                bitmap.text = wanted;
            }
        } else if let Some(mut text) = text
            && text.0 != wanted
        {
            text.0 = wanted;
        }
    }
    for (fill, mut node) in &mut fills {
        let fraction = fill.0.slider_value(&settings).unwrap_or(0.0);
        let width = px(TUBE.width() * fraction);
        if node.width != width {
            node.width = width;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn on_options_activate(
    activate: On<Activate>,
    actions: Query<&OptionsAction>,
    mut commands: Commands,
    mut settings: ResMut<GameSettings>,
    mut tab: ResMut<CurrentTab>,
    mut next_state: ResMut<NextState<AppState>>,
    confirm: Option<Res<ResolutionConfirm>>,
    mut pending: Option<ResMut<PendingResolution>>,
    dialogs: Query<Entity, With<ResolutionDialog>>,
    sounds: Option<Res<MenuSounds>>,
    fonts: Option<Res<ArcanumFonts>>,
) {
    let Ok(&action) = actions.get(activate.entity) else { return };
    // While the confirmation is up, only its own buttons respond.
    if confirm.is_some() && !matches!(action, OptionsAction::KeepResolution | OptionsAction::RevertResolution) {
        return;
    }
    if let Some(sounds) = &sounds {
        sounds.play_click(&mut commands);
    }
    match action {
        OptionsAction::Tab(t) => {
            if tab.0 != t {
                tab.0 = t;
                commands.insert_resource(RebuildOptions);
            }
        }
        OptionsAction::Done => {
            let fonts = fonts.map(|f| f.clone()).unwrap_or_default();
            leave(&mut commands, &mut settings, pending.as_deref(), &mut next_state, &fonts);
        }
        // The resolution arrows only change the pending choice; Done applies it.
        OptionsAction::Step { option: OptionId::Resolution, delta } => {
            if let Some(pending) = &mut pending {
                pending.0 = step_resolution(pending.0, delta);
            }
        }
        OptionsAction::Step { option, delta } => option.step(&mut settings, delta),
        OptionsAction::KeepResolution => {
            close_dialog(&mut commands, &dialogs);
            next_state.set(AppState::MainMenu);
        }
        OptionsAction::RevertResolution => {
            if let Some(confirm) = &confirm {
                revert_resolution(&mut settings, pending.as_deref_mut(), confirm.previous);
            }
            close_dialog(&mut commands, &dialogs);
            next_state.set(AppState::MainMenu);
        }
    }
}

/// Done: returns to the main menu, or first applies a newly picked resolution and asks the
/// player to confirm it.
fn leave(
    commands: &mut Commands,
    settings: &mut GameSettings,
    pending: Option<&PendingResolution>,
    next_state: &mut NextState<AppState>,
    fonts: &ArcanumFonts,
) {
    match pending.map(|p| p.0) {
        Some(target) if target != settings.resolution => {
            commands.insert_resource(ResolutionConfirm {
                previous: settings.resolution,
                timer: Timer::from_seconds(RESOLUTION_CONFIRM_SECS, TimerMode::Once),
            });
            settings.resolution = target;
            spawn_resolution_dialog(commands, fonts);
        }
        _ => next_state.set(AppState::MainMenu),
    }
}

/// Puts the resolution back, and the pending choice with it so the slot shows what is applied.
fn revert_resolution(settings: &mut GameSettings, pending: Option<&mut PendingResolution>, previous: DisplayResolution) {
    settings.resolution = previous;
    if let Some(pending) = pending {
        pending.0 = previous;
    }
}

fn close_dialog(commands: &mut Commands, dialogs: &Query<Entity, With<ResolutionDialog>>) {
    commands.remove_resource::<ResolutionConfirm>();
    for dialog in dialogs {
        commands.entity(dialog).despawn();
    }
}

/// Text shown in the resolution confirmation.
pub const KEEP_RESOLUTION_TITLE: &str = "Confirm Resolution";
pub const KEEP_LABEL: &str = "Keep";
pub const REVERT_LABEL: &str = "Revert";

fn countdown_text(seconds: f32) -> String {
    format!("Reverting in {} seconds", seconds.ceil() as u32)
}

fn spawn_resolution_dialog(commands: &mut Commands, fonts: &ArcanumFonts) {
    let red = HoverText { normal: TEXT_COLOR, hover: HOVER_COLOR };
    commands
        .spawn((
            ResolutionDialog,
            DespawnOnExit(AppState::Options),
            GlobalZIndex(10),
            // Covers the whole window, dimming the screen and blocking clicks behind it.
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    BackgroundColor(Color::srgb_u8(40, 26, 18)),
                    BorderColor::all(Color::srgb_u8(170, 130, 60)),
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(10.0),
                        padding: UiRect::all(px(16.0)),
                        border: UiRect::all(px(2.0)),
                        ..default()
                    },
                ))
                .with_children(|panel| {
                    let mut title = panel.spawn(TextColor(HOVER_COLOR));
                    fonts.insert_text(&mut title, KEEP_RESOLUTION_TITLE, UiFont::Large);
                    let mut countdown = panel.spawn((ResolutionCountdown, TextColor(VALUE_COLOR)));
                    fonts.insert_text(&mut countdown, &countdown_text(RESOLUTION_CONFIRM_SECS), UiFont::Small);
                    panel.spawn(Node { column_gap: px(40.0), ..default() }).with_children(|row| {
                        let mut keep = row.spawn(text_button(red, OptionsAction::KeepResolution));
                        fonts.insert_text(&mut keep, KEEP_LABEL, UiFont::Large);
                        let mut revert = row.spawn(text_button(red, OptionsAction::RevertResolution));
                        fonts.insert_text(&mut revert, REVERT_LABEL, UiFont::Large);
                    });
                });
        });
}

/// Counts down the resolution confirmation; when time runs out, reverts and returns to the
/// main menu. Uses wall-clock time, so the countdown keeps going even if the game clock is
/// paused or slowed.
#[allow(clippy::too_many_arguments)]
fn tick_resolution_confirm(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut confirm: ResMut<ResolutionConfirm>,
    mut settings: ResMut<GameSettings>,
    mut pending: Option<ResMut<PendingResolution>>,
    dialogs: Query<Entity, With<ResolutionDialog>>,
    mut countdown: Query<(Option<&mut BitmapText>, Option<&mut Text>), With<ResolutionCountdown>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    confirm.timer.tick(time.delta());
    if confirm.timer.is_finished() {
        revert_resolution(&mut settings, pending.as_deref_mut(), confirm.previous);
        close_dialog(&mut commands, &dialogs);
        next_state.set(AppState::MainMenu);
        return;
    }
    let wanted = countdown_text(confirm.timer.remaining_secs());
    for (bitmap, text) in &mut countdown {
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

/// Escape answers the confirmation with Revert; otherwise it acts like Done.
#[allow(clippy::too_many_arguments)]
fn options_escape(
    mut commands: Commands,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    confirm: Option<Res<ResolutionConfirm>>,
    mut settings: ResMut<GameSettings>,
    mut pending: Option<ResMut<PendingResolution>>,
    dialogs: Query<Entity, With<ResolutionDialog>>,
    mut next_state: ResMut<NextState<AppState>>,
    fonts: Option<Res<ArcanumFonts>>,
) {
    if !keys.is_some_and(|k| k.just_pressed(KeyCode::Escape)) {
        return;
    }
    if let Some(confirm) = confirm {
        revert_resolution(&mut settings, pending.as_deref_mut(), confirm.previous);
        close_dialog(&mut commands, &dialogs);
        next_state.set(AppState::MainMenu);
    } else {
        let fonts = fonts.map(|f| f.clone()).unwrap_or_default();
        leave(&mut commands, &mut settings, pending.as_deref(), &mut next_state, &fonts);
    }
}

/// True once the Options screen has been built.
pub fn options_ready(slots: Query<(), With<OptionSlot>>) -> bool {
    !slots.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_hold_the_agreed_settings() {
        use OptionId::*;
        assert_eq!(OptionsTab::Game.options(), [AutoAttack, CombatTaunts]);
        assert_eq!(OptionsTab::Video.options(), [Brightness, TextDuration, Floats, FloatSpeed, Fullscreen, Resolution]);
        assert_eq!(OptionsTab::Sound.options(), [Effects, Voice, Music]);
        // Every tab fits the 2x4 grid.
        assert!(OptionsTab::ALL.iter().all(|t| t.options().len() <= 8));
    }

    #[test]
    fn sliders_step_by_a_tenth_and_clamp() {
        let mut s = GameSettings { music_volume: 0.95, ..default() };
        OptionId::Music.step(&mut s, 1);
        assert_eq!(s.music_volume, 1.0);
        OptionId::Music.step(&mut s, 1);
        assert_eq!(s.music_volume, 1.0);
        for _ in 0..12 {
            OptionId::Music.step(&mut s, -1);
        }
        assert_eq!(s.music_volume, 0.0);
        OptionId::Music.step(&mut s, 1);
        OptionId::Music.step(&mut s, 1);
        OptionId::Music.step(&mut s, 1);
        assert_eq!(s.music_volume, 0.3, "no float drift");
    }

    #[test]
    fn toggles_flip_either_way() {
        let mut s = GameSettings::default();
        let before = s.auto_attack;
        OptionId::AutoAttack.step(&mut s, 1);
        assert_eq!(s.auto_attack, !before);
        OptionId::AutoAttack.step(&mut s, -1);
        assert_eq!(s.auto_attack, before);
    }

    #[test]
    fn choices_wrap() {
        let mut s = GameSettings { floats: Floats::Never, ..default() };
        OptionId::Floats.step(&mut s, -1);
        assert_eq!(s.floats, Floats::Verbose);
        OptionId::Floats.step(&mut s, 1);
        assert_eq!(s.floats, Floats::Never);
    }

    #[test]
    fn resolution_cycles_presets_and_custom_sizes_join_in_order() {
        let mut s = GameSettings { resolution: DisplayResolution::Fhd, ..default() };
        OptionId::Resolution.step(&mut s, 1);
        assert_eq!(s.resolution, DisplayResolution::Qhd);
        OptionId::Resolution.step(&mut s, -2);
        assert_eq!(s.resolution, DisplayResolution::HdPlus);

        let custom = DisplayResolution::Custom(1366, 768);
        assert_eq!(step_resolution(custom, 1), DisplayResolution::HdPlus);
        assert_eq!(step_resolution(custom, -1), DisplayResolution::Hd);
        // Wraps at both ends.
        assert_eq!(step_resolution(DisplayResolution::Uhd4k, 1), DisplayResolution::Hd);
        assert_eq!(step_resolution(DisplayResolution::Hd, -1), DisplayResolution::Uhd4k);
    }

    #[test]
    fn labels_fall_back_to_english() {
        let labels = OptionsLabels::resolve(None, None, None, None);
        assert_eq!(labels.tab(OptionsTab::Video), "Video");
        assert_eq!(labels.done(), "Done");
        assert_eq!(labels.name(OptionId::CombatTaunts), "Combat Taunts");
        let s = GameSettings { combat_taunts: true, floats: Floats::Verbose, ..default() };
        assert_eq!(labels.value(OptionId::CombatTaunts, &s), "On");
        assert_eq!(labels.value(OptionId::Floats, &s), "Verbose");
        assert_eq!(labels.value(OptionId::Resolution, &s), "1920 x 1080");
        assert_eq!(labels.value(OptionId::Music, &s), "");
    }
}
