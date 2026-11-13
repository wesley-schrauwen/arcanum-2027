//! Menu music, interface sound effects and voice, loaded through the VFS.
//!
//! The Sound settings are live: Music scales the menu music, Effects scales interface sounds,
//! and Voice scales anything tagged [`VoiceLine`]. Changes apply to sounds already playing.

use bevy::audio::Volume;
use bevy::prelude::*;

use crate::resource_pack::paths::sound;
use crate::settings::GameSettings;
use crate::vfs_asset::VfsResource;

/// The original plays the menu theme at 50% (`/vol:50` in `sound/schemelist.mes`).
pub const MENU_MUSIC_VOLUME: f32 = 0.5;

/// The menu music's volume: the theme's own level scaled by the player's Music setting.
pub fn menu_music_volume(settings: &GameSettings) -> Volume {
    Volume::Linear(MENU_MUSIC_VOLUME * settings.music_volume.clamp(0.0, 1.0))
}

/// Marks the looping menu music entity.
#[derive(Component)]
pub struct MenuMusic;

/// Volume for sound effects at the player's Effects setting.
pub fn effects_volume(settings: &GameSettings) -> Volume {
    Volume::Linear(settings.effects_volume.clamp(0.0, 1.0))
}

/// Volume for voice lines at the player's Voice setting.
pub fn voice_volume(settings: &GameSettings) -> Volume {
    Volume::Linear(settings.voice_volume.clamp(0.0, 1.0))
}

/// Marks a sound effect; its volume follows the Effects setting while it plays.
#[derive(Component)]
pub struct SoundEffect;

/// Marks spoken dialogue; its volume follows the Voice setting while it plays. Nothing plays
/// voice yet: dialogue should spawn its lines with [`play_voice`].
#[derive(Component)]
pub struct VoiceLine;

/// Interface sounds. A sound is `None` when its file is missing from the VFS.
#[derive(Resource, Clone, Debug)]
pub struct MenuSounds {
    pub hover: Option<Handle<AudioSource>>,
    pub click: Option<Handle<AudioSource>>,
    /// Picture buttons (the Options arrows): one sound on press, one on release.
    pub button_press: Option<Handle<AudioSource>>,
    pub button_release: Option<Handle<AudioSource>>,
    /// The Effects volume, kept in step with [`GameSettings`].
    pub volume: Volume,
}

impl Default for MenuSounds {
    fn default() -> Self {
        Self { hover: None, click: None, button_press: None, button_release: None, volume: Volume::Linear(1.0) }
    }
}

impl MenuSounds {
    pub fn play_hover(&self, commands: &mut Commands) {
        self.play_once(commands, self.hover.as_ref());
    }

    pub fn play_click(&self, commands: &mut Commands) {
        self.play_once(commands, self.click.as_ref());
    }

    pub fn play_button_press(&self, commands: &mut Commands) {
        self.play_once(commands, self.button_press.as_ref());
    }

    pub fn play_button_release(&self, commands: &mut Commands) {
        self.play_once(commands, self.button_release.as_ref());
    }

    /// Spawns a one-shot effect at the Effects volume that despawns itself when it finishes.
    fn play_once(&self, commands: &mut Commands, sound: Option<&Handle<AudioSource>>) {
        if let Some(sound) = sound {
            commands.spawn((
                SoundEffect,
                AudioPlayer::new(sound.clone()),
                PlaybackSettings::DESPAWN.with_volume(self.volume),
            ));
        }
    }
}

/// Plays a voice line once at the Voice volume.
pub fn play_voice(commands: &mut Commands, settings: &GameSettings, line: Handle<AudioSource>) -> Entity {
    commands
        .spawn((VoiceLine, AudioPlayer::new(line), PlaybackSettings::DESPAWN.with_volume(voice_volume(settings))))
        .id()
}

/// Loads the menu sounds and starts the menu music. Add after `DefaultPlugins`.
pub struct MenuAudioPlugin;

impl Plugin for MenuAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuSounds>()
            .add_systems(Startup, setup_audio)
            .add_systems(Update, apply_volumes.run_if(resource_changed::<GameSettings>));
    }
}

fn setup_audio(
    mut commands: Commands,
    mut sounds: ResMut<MenuSounds>,
    vfs: Option<Res<VfsResource>>,
    assets: Res<AssetServer>,
    settings: Option<Res<GameSettings>>,
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
    if exists(sound::BUTTON_PRESS) {
        sounds.button_press = Some(assets.load(sound::BUTTON_PRESS));
    }
    if exists(sound::BUTTON_RELEASE) {
        sounds.button_release = Some(assets.load(sound::BUTTON_RELEASE));
    }
    if exists(sound::MAIN_MENU_MUSIC) {
        commands.spawn((
            MenuMusic,
            AudioPlayer::new(assets.load(sound::MAIN_MENU_MUSIC)),
            PlaybackSettings::LOOP.with_volume(menu_music_volume(&settings.map(|s| s.clone()).unwrap_or_default())),
        ));
    }
}

type Playing<'a> = (&'a mut PlaybackSettings, Option<&'a mut AudioSink>);

/// Follows changes to the Sound settings: adjusts sounds already playing, sounds queued but not
/// started yet, and the volume new interface sounds start at.
fn apply_volumes(
    settings: Res<GameSettings>,
    mut sounds: ResMut<MenuSounds>,
    mut music: Query<Playing, With<MenuMusic>>,
    mut effects: Query<Playing, (With<SoundEffect>, Without<MenuMusic>)>,
    mut voices: Query<Playing, (With<VoiceLine>, Without<MenuMusic>, Without<SoundEffect>)>,
) {
    let effects_level = effects_volume(&settings);
    if sounds.volume != effects_level {
        sounds.volume = effects_level;
    }
    set_volume(music.iter_mut(), menu_music_volume(&settings));
    set_volume(effects.iter_mut(), effects_level);
    set_volume(voices.iter_mut(), voice_volume(&settings));
}

fn set_volume<'a>(sounds: impl Iterator<Item = (Mut<'a, PlaybackSettings>, Option<Mut<'a, AudioSink>>)>, volume: Volume) {
    for (mut playback, sink) in sounds {
        if playback.volume != volume {
            playback.volume = volume;
        }
        if let Some(mut sink) = sink
            && sink.volume() != volume
        {
            sink.set_volume(volume);
        }
    }
}
