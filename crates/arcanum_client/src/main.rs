use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Arcanum Reimplementation (Bevy)".into(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup_scene)
        .run();
}

fn setup_scene(mut commands: Commands) {
    // Spawn a basic 2D camera to anchor our viewport
    commands.spawn(Camera2d);
}