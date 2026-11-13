//! The loading screen, shown on the way into the game (New Game now, Load Game later).
//!
//! It shows one of the original's three "Loading Arcanum..." pictures at random (800x400, words
//! painted in), centred on black, with the original's pocket-watch indicator
//! (`LoadIndicator.ART`, its hand going round once every 8 s) in the bottom-right corner of the window, where the original
//! kept it.
//!
//! To use it, insert a [`LoadRequest`] naming the screen to go to and the assets to wait for,
//! then switch to [`AppState::Loading`]. The screen waits until every asset has loaded (or
//! failed, with a warning) and at least [`LoadRequest::min_secs`] have passed (default
//! [`MIN_LOADING_SECS`]), then moves on.
//! [`LoadingProgress`] reports how far it has got, for a progress bar later.

use std::time::{SystemTime, UNIX_EPOCH};

use bevy::asset::{LoadState, UntypedHandle};
use bevy::prelude::*;

use crate::art_loader::ArtFrames;
use crate::fonts::ORIGINAL_SCALE;
use crate::main_menu::AppState;
use crate::resource_pack::paths::{backgrounds, interface};

/// Shortest time the loading screen stays up, so it never just flickers.
pub const MIN_LOADING_SECS: f32 = 1.5;

// Layout in original pixels on the 800x600 screen; scaled by ORIGINAL_SCALE to the 1080 baseline.
const PICTURE_SIZE: Vec2 = Vec2::new(800.0, 400.0);
/// The picture sits in the middle of the 600-pixel-tall screen.
const PICTURE_TOP: f32 = (600.0 - 400.0) / 2.0;
const INDICATOR_SIZE: Vec2 = Vec2::new(40.0, 50.0);
/// Seconds for the pocket watch's hand to go once round (all 30 frames). The file's own rate
/// (15 fps) would take 2 s; this is slowed down on purpose.
pub const INDICATOR_CYCLE_SECS: f32 = 8.0;

/// Gap between the pocket watch and the window's bottom-right corner.
const INDICATOR_MARGIN: f32 = 10.0;

/// What to load, and where to go when it is done. Insert before entering [`AppState::Loading`].
#[derive(Resource, Debug, Clone)]
pub struct LoadRequest {
    pub next: AppState,
    pub assets: Vec<UntypedHandle>,
    /// Shortest time to stay on the loading screen.
    pub min_secs: f32,
}

impl LoadRequest {
    pub fn new(next: AppState) -> Self {
        Self { next, assets: Vec::new(), min_secs: MIN_LOADING_SECS }
    }

    /// Stays up for at least `secs`.
    pub fn min_secs(mut self, secs: f32) -> Self {
        self.min_secs = secs;
        self
    }

    /// Also waits for `handle`.
    pub fn with(mut self, handle: impl Into<UntypedHandle>) -> Self {
        self.assets.push(handle.into());
        self
    }
}

/// How far the current load has got.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct LoadingProgress {
    /// Assets finished (loaded or failed).
    pub done: usize,
    pub total: usize,
    /// Seconds since the loading screen appeared (wall clock).
    pub elapsed: f32,
}

impl LoadingProgress {
    /// `0.0..=1.0`; 1.0 when there is nothing to load.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 { 1.0 } else { self.done as f32 / self.total as f32 }
    }

    /// Everything is loaded and the screen has been up for at least `min_secs`.
    pub fn is_finished(&self, min_secs: f32) -> bool {
        self.done >= self.total && self.elapsed >= min_secs
    }
}

/// Root of the loading screen.
#[derive(Component)]
pub struct LoadingScreen;

/// The "Loading Arcanum..." picture; which of the three is in [`LoadingScreen`]'s image.
#[derive(Component, Debug)]
pub struct LoadingPicture(pub usize);

/// The turning pocket watch.
#[derive(Component)]
pub struct LoadingIndicator(pub Handle<ArtFrames>);

/// Wall-clock time the loading screen appeared.
#[derive(Resource)]
struct LoadingStarted(f32);

pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Loading), enter_loading)
            .add_systems(OnExit(AppState::Loading), exit_loading)
            .add_systems(
                Update,
                (track_progress, animate_indicator).run_if(in_state(AppState::Loading)),
            );
    }
}

/// One of the three pictures, varying from run to run.
fn pick_picture() -> usize {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    nanos as usize % backgrounds::LOADING.len()
}

fn px(v: f32) -> Val {
    Val::Px(v * ORIGINAL_SCALE)
}

fn enter_loading(
    mut commands: Commands,
    assets: Res<AssetServer>,
    time: Res<Time<Real>>,
    request: Option<Res<LoadRequest>>,
) {
    if request.is_none() {
        warn!("entered the loading screen without a LoadRequest; returning to the main menu afterwards");
        commands.insert_resource(LoadRequest::new(AppState::MainMenu));
    }
    commands.insert_resource(LoadingStarted(time.elapsed_secs()));
    commands.insert_resource(LoadingProgress::default());

    let picture = pick_picture();
    let indicator = assets.load(interface::LOAD_INDICATOR);
    commands
        .spawn((
            LoadingScreen,
            DespawnOnExit(AppState::Loading),
            // Covers the menu background, which stays loaded behind every screen.
            BackgroundColor(Color::BLACK),
            GlobalZIndex(5),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ))
        .with_children(|screen| {
            screen.spawn((
                LoadingPicture(picture),
                ImageNode::new(assets.load(backgrounds::LOADING[picture])),
                Node {
                    position_type: PositionType::Absolute,
                    top: px(PICTURE_TOP),
                    left: Val::Percent(50.0),
                    margin: UiRect::left(px(-PICTURE_SIZE.x / 2.0)),
                    width: px(PICTURE_SIZE.x),
                    height: px(PICTURE_SIZE.y),
                    ..default()
                },
            ));
            screen.spawn((
                LoadingIndicator(indicator),
                ImageNode::default(),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(INDICATOR_MARGIN),
                    bottom: px(INDICATOR_MARGIN),
                    width: px(INDICATOR_SIZE.x),
                    height: px(INDICATOR_SIZE.y),
                    ..default()
                },
            ));
        });
}

fn exit_loading(mut commands: Commands) {
    commands.remove_resource::<LoadRequest>();
    commands.remove_resource::<LoadingStarted>();
    commands.remove_resource::<LoadingProgress>();
}

/// Counts finished assets and moves on once everything is done and the minimum time has passed.
fn track_progress(
    assets: Res<AssetServer>,
    time: Res<Time<Real>>,
    request: Option<Res<LoadRequest>>,
    started: Option<Res<LoadingStarted>>,
    progress: Option<ResMut<LoadingProgress>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut reported: Local<Vec<bevy::asset::UntypedAssetId>>,
) {
    let (Some(request), Some(started), Some(mut progress)) = (request, started, progress) else { return };
    let mut done = 0;
    for handle in &request.assets {
        match assets.get_load_state(handle.id()) {
            Some(LoadState::Failed(err)) => {
                if !reported.contains(&handle.id()) {
                    warn!("loading screen: an asset failed to load ({err}); continuing without it");
                    reported.push(handle.id());
                }
                done += 1;
            }
            // Not tracked by the asset server (added directly): nothing to wait for.
            None => done += 1,
            Some(_) if assets.is_loaded_with_dependencies(handle.id()) => done += 1,
            Some(_) => {}
        }
    }
    let now = LoadingProgress { done, total: request.assets.len(), elapsed: time.elapsed_secs() - started.0 };
    if *progress != now {
        *progress = now;
    }
    if now.is_finished(request.min_secs) {
        reported.clear();
        next_state.set(request.next);
    }
}

/// Plays the pocket watch, one full turn of the hand every [`INDICATOR_CYCLE_SECS`].
fn animate_indicator(
    time: Res<Time<Real>>,
    frames: Res<Assets<ArtFrames>>,
    started: Option<Res<LoadingStarted>>,
    mut indicators: Query<(&LoadingIndicator, &mut ImageNode)>,
) {
    let elapsed = started.map_or(0.0, |s| time.elapsed_secs() - s.0);
    for (indicator, mut image) in &mut indicators {
        let Some(set) = frames.get(&indicator.0) else { continue };
        let index = indicator_frame(elapsed, set.frames.len());
        if let Some(frame) = set.frame(index)
            && image.image != *frame
        {
            image.image = frame.clone();
        }
    }
}

/// The pocket-watch frame to show `elapsed` seconds in, with `count` frames per turn.
pub fn indicator_frame(elapsed: f32, count: usize) -> usize {
    let count = count.max(1);
    let turn = (elapsed / INDICATOR_CYCLE_SECS).fract();
    ((turn * count as f32) as usize).min(count - 1)
}

/// True once the loading picture is on screen (for screenshots).
pub fn loading_ready(pictures: Query<&ImageNode, With<LoadingPicture>>, images: Res<Assets<Image>>) -> bool {
    pictures.iter().any(|p| images.get(&p.image).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finishes_only_when_loaded_and_shown_long_enough() {
        let p = |done, total, elapsed| LoadingProgress { done, total, elapsed };
        let min = MIN_LOADING_SECS;
        assert!(!p(1, 2, 5.0).is_finished(min), "still loading");
        assert!(!p(2, 2, 0.5).is_finished(min), "too soon");
        assert!(p(2, 2, min).is_finished(min));
        assert!(p(0, 0, min).is_finished(min), "nothing to load");
        assert!(!p(0, 0, min).is_finished(60.0));
        assert_eq!(p(1, 4, 0.0).fraction(), 0.25);
        assert_eq!(p(0, 0, 0.0).fraction(), 1.0);
    }

    #[test]
    fn the_watch_goes_round_once_every_eight_seconds() {
        assert_eq!(indicator_frame(0.0, 30), 0);
        assert_eq!(indicator_frame(4.0, 30), 15, "halfway round at 4 s");
        assert_eq!(indicator_frame(7.99, 30), 29);
        assert_eq!(indicator_frame(8.0, 30), 0, "back to the start at 8 s");
        assert_eq!(indicator_frame(12.0, 30), 15);
        assert_eq!(indicator_frame(1.0, 0), 0);
    }

    #[test]
    fn layout_fits_the_original_screen() {
        // The watch fits in the black band under the picture, so it never covers it.
        assert!(INDICATOR_MARGIN + INDICATOR_SIZE.y <= 600.0 - (PICTURE_TOP + PICTURE_SIZE.y));
        assert!(pick_picture() < backgrounds::LOADING.len());
    }
}
