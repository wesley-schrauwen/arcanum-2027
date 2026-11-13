//! Menu music and interface sound effects, loaded through the VFS.

use bevy::audio::Volume;
use bevy::prelude::*;

use crate::resource_pack::paths::sound;
use crate::vfs_asset::VfsResource;

/// The original plays the menu theme at 50% (`/vol:50` in `sound/schemelist.mes`).
pub const MENU_MUSIC_VOLUME: f32 = 0.5;

/// Marks the looping menu music entity.
#[derive(Component)]
pub struct MenuMusic;

/// Interface sounds. A sound is `None` when its file is missing from the VFS.
#[derive(Resource, Default, Clone, Debug)]
pub struct MenuSounds {
    pub hover: Option<Handle<AudioSource>>,
    pub click: Option<Handle<AudioSource>>,
}

impl MenuSounds {
    pub fn play_hover(&self, commands: &mut Commands) {
        play_once(commands, self.hover.as_ref());
    }

    pub fn play_click(&self, commands: &mut Commands) {
        play_once(commands, self.click.as_ref());
    }
}

/// Spawns a one-shot sound that despawns itself when it finishes.
fn play_once(commands: &mut Commands, sound: Option<&Handle<AudioSource>>) {
    if let Some(sound) = sound {
        commands.spawn((AudioPlayer::new(sound.clone()), PlaybackSettings::DESPAWN));
    }
}

/// Loads the menu sounds and starts the menu music. Add after `DefaultPlugins`.
pub struct MenuAudioPlugin;

impl Plugin for MenuAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuSounds>().add_systems(Startup, setup_audio);
    }
}

fn setup_audio(
    mut commands: Commands,
    mut sounds: ResMut<MenuSounds>,
    vfs: Option<Res<VfsResource>>,
    assets: Res<AssetServer>,
) {
    let exists = |path: &str| {
        let found = vfs.as_ref().is_some_and(|vfs| vfs.0.read().is_ok_and(|v| v.exists(path)));
        if !found {
            warn!("{path} not found in the VFS; skipping");
        }
        found
    };
    if exists(sound::MENU_HOVER) {
        sounds.hover = Some(assets.load(sound::MENU_HOVER));
    }
    if exists(sound::MENU_CLICK) {
        sounds.click = Some(assets.load(sound::MENU_CLICK));
    }
    if exists(sound::MAIN_MENU_MUSIC) {
        commands.spawn((
            MenuMusic,
            AudioPlayer::new(assets.load(sound::MAIN_MENU_MUSIC)),
            PlaybackSettings::LOOP.with_volume(Volume::Linear(MENU_MUSIC_VOLUME)),
        ));
    }
}
