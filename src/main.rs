use bevy::{
    app::{App, FixedUpdate, Startup, Update},
    asset::{AssetServer, Assets},
    input::ButtonInput,
    math::{Quat, Vec2, Vec2Swizzles, Vec3, Vec3Swizzles},
    prelude::{
        default, Camera2dBundle, Changed, Circle, Commands, Component, Deref, DerefMut, Event,
        EventReader, EventWriter, IntoSystemConfigs, MouseButton, Query, Res, ResMut, Resource,
        With,
    },
    render::{camera::Camera, color::Color, mesh::Mesh},
    sprite::{
        ColorMaterial, MaterialMesh2dBundle, SpriteBundle, SpriteSheetBundle, TextureAtlas,
        TextureAtlasLayout,
    },
    text::{TextSection, TextStyle},
    time::{Time, Timer, TimerMode},
    transform::components::{GlobalTransform, Transform},
    ui::{node_bundles::TextBundle, PositionType, Style, Val},
    window::{PrimaryWindow, Window},
    DefaultPlugins,
};

const BOW_STARTING_POSITION: Vec3 = Vec3::new(0.0, -50.0, 1.0);
const BOW_FULL_PULL_TIME: f32 = 1.5;

const SCOREBOARD_FONT_SIZE: f32 = 20.0;
const SCOREBOARD_TEXT_PADDING: Val = Val::Px(5.0);

const TEXT_COLOR: Color = Color::rgb(0.5, 0.5, 1.0);
const SCORE_COLOR: Color = Color::rgb(1.0, 0.5, 0.5);


fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(Mouse(Vec2::ZERO))
        .insert_resource(Scoreboard { score: 0 })
        .insert_resource(G(5.))
        .add_systems(Startup, (setup).chain())
        .add_systems(Update, (animate_sprite, move_arrows).chain())
        .add_systems(
            FixedUpdate,
            (
                update_mouse,
                shoot_bow,
                shoot_arrow,
                move_bow_cursor,
                rotate_bow,
            )
                .chain(),
        )
        .add_event::<ArrowShotEvent>()
        .run();
}

#[derive(Resource, Deref, DerefMut)]
struct G(f32);

#[derive(Resource, Deref, DerefMut)]
struct Mouse(Vec2);

#[derive(Resource)]
struct Scoreboard {
    score: u32,
}

#[derive(Component)]
struct ScoreboardUi;

#[derive(Component)]
struct Bow;

#[derive(Component, Deref, DerefMut)]
struct Fixed(bool);

#[derive(Component)]
struct Arrow;

#[derive(Event, Default)]
struct ArrowShotEvent {
    pos: Vec2,
    angle: Quat,
    velocity: Vec2,
}

#[derive(Component)]
struct Pos(Vec2);

#[derive(Component, Deref, DerefMut)]
struct Velocity(Vec2);

// Animation
#[derive(Component)]
struct AnimationIndices {
    first: usize,
    last: usize,
}

#[derive(Component, Deref, DerefMut)]
struct AnimationTimer(Timer);

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    asset_server: Res<AssetServer>,
) {
    commands.spawn(Camera2dBundle::default());

    // Bow
    let texture = asset_server.load("bow/bow-atlas.png");
    let size = 190.;
    let layout = TextureAtlasLayout::from_grid(Vec2::new(size / 3., size / 3.), 3, 3, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);
    // Use only the subset of sprites in the sheet that make up the run animation
    let animation_indices = AnimationIndices { first: 0, last: 7 };
    commands.spawn((
        SpriteSheetBundle {
            texture,
            atlas: TextureAtlas {
                layout: texture_atlas_layout,
                index: animation_indices.first,
            },
            ..default()
        },
        animation_indices,
        Bow,
        AnimationTimer(Timer::from_seconds(
            BOW_FULL_PULL_TIME / 8.,
            TimerMode::Once,
        )),
        Fixed(false),
    ));

    // Scoreboard
    commands.spawn((
        ScoreboardUi,
        TextBundle::from_sections([
            TextSection::new(
                "Score: ",
                TextStyle {
                    font_size: SCOREBOARD_FONT_SIZE,
                    color: TEXT_COLOR,
                    ..default()
                },
            ),
            TextSection::from_style(TextStyle {
                font_size: SCOREBOARD_FONT_SIZE,
                color: SCORE_COLOR,
                ..default()
            }),
        ])
        .with_style(Style {
            position_type: PositionType::Absolute,
            top: SCOREBOARD_TEXT_PADDING,
            left: SCOREBOARD_TEXT_PADDING,
            ..default()
        }),
    ));
}

fn animate_sprite(
    time: Res<Time>,
    mut query: Query<
        (
            &AnimationIndices,
            &mut AnimationTimer,
            &mut TextureAtlas,
            &Fixed,
        ),
        With<Bow>,
    >,
) {
    for (indices, mut timer, mut atlas, fixed) in &mut query {
        if **fixed {
            timer.tick(time.delta());
            if timer.just_finished() {
                if atlas.index < indices.last {
                    atlas.index += 1;
                    timer.reset();
                };
            }
        } else {
            atlas.index = indices.first;
            timer.reset();
        }
    }
}

fn update_mouse(
    mut mouse: ResMut<Mouse>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
) {
    let win = window.single();
    let (camera, camera_transform) = camera_q.single();

    win.cursor_position()
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor))
        .and_then(|cursor_position| {
            mouse.x = cursor_position.x;
            mouse.y = cursor_position.y;
            Some(())
        });
}

fn shoot_bow(
    buttons: Res<ButtonInput<MouseButton>>,
    bow: Query<(&Transform, &Fixed), With<Bow>>,
    mut shot_event_writer: EventWriter<ArrowShotEvent>,
) {
    let (tr, fixed) = bow.single();

    if **fixed && buttons.just_released(MouseButton::Left) {
        shot_event_writer.send(ArrowShotEvent {
            pos: tr.translation.xy(),
            angle: tr.rotation,
            // 50 px per seconds, velocity should be relative to the time it needs to escape the
            //    screen
            velocity: Vec2::new(50.0, 0.0),
        });
    }
}

fn move_bow_cursor(
    mouse: Res<Mouse>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut bow: Query<(&mut Transform, &mut Fixed), With<Bow>>,
) {
    let (mut tr, mut fixed) = bow.single_mut();

    **fixed = buttons.pressed(MouseButton::Left);

    if !**fixed {
        tr.translation.x = mouse.x;
        tr.translation.y = mouse.y;
    }
}

fn rotate_bow(mouse: Res<Mouse>, mut bow: Query<(&mut Transform, &Fixed), With<Bow>>) {
    let ms = **mouse;
    let (mut tr, fixed) = bow.single_mut();

    if **fixed {
        let pos = tr.translation;

        let dir_to_mouse = (ms - pos.xy()).normalize();
        let angle = dir_to_mouse.y.atan2(dir_to_mouse.x) - std::f32::consts::PI;
        let rot = Quat::from_rotation_z(angle);

        tr.rotation = rot;
    }
}

fn shoot_arrow(
    mut ev_shoot: EventReader<ArrowShotEvent>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    for ev in ev_shoot.read() {
        commands.spawn((
            Arrow,
            SpriteBundle {
                texture: asset_server.load("bow/arrow.png"),
                transform: Transform::from_translation(ev.pos.extend(0.0)).with_rotation(ev.angle),
                ..default()
            },
            Velocity(ev.velocity),
        ));
    }
}

fn move_arrows(time: Res<Time>, g: Res<G>, mut arrows: Query<(&mut Transform, &Velocity), With<Arrow>>) {
    for (mut tr, vel) in &mut arrows {
        tr.translation.x += vel.x * time.delta_seconds();
        tr.translation.y -= **g;
    }
}
