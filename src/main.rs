use bevy::{
    app::{App, FixedUpdate, Startup, Update},
    asset::{AssetServer, Assets},
    hierarchy::BuildChildren,
    input::ButtonInput,
    math::{FloatExt, Quat, Rect, Vec2, Vec2Swizzles, Vec3, Vec3Swizzles},
    prelude::{
        default, Camera2dBundle, Changed, Circle, Commands, Component, Deref, DerefMut, Entity,
        Event, EventReader, EventWriter, Gizmos, IntoSystemConfigs, MouseButton, Query, Res,
        ResMut, Resource, With,
    },
    reflect::Reflect,
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
use bevy_bow::{ProgressBar, ProgressBarBundle, ProgressBarMaterial, ProgressBarPlugin};
use bevy_editor_pls::EditorPlugin;
use bevy_inspector_egui::quick::{ResourceInspectorPlugin, WorldInspectorPlugin};

const BOW_FULL_PULL_TIME: f32 = 1.;
const BOW_SIZE: f32 = 190. / 3.;

const SCOREBOARD_FONT_SIZE: f32 = 20.0;
const SCOREBOARD_TEXT_PADDING: Val = Val::Px(5.0);

const TEXT_COLOR: Color = Color::rgb(0.5, 0.5, 1.0);
const SCORE_COLOR: Color = Color::rgb(1.0, 0.5, 0.5);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(ProgressBarPlugin)
        .add_plugins(WorldInspectorPlugin::new())
        .insert_resource(Mouse(Vec2::ZERO))
        .insert_resource(Scoreboard { score: 0 })
        .insert_resource(G(18.))
        .add_plugins(ResourceInspectorPlugin::<G>::new())
        .add_systems(Startup, (setup).chain())
        .add_systems(
            Update,
            (draw_bow, draw_bow_area, move_arrows, rotate_arrows).chain(),
        )
        .add_systems(
            FixedUpdate,
            (
                update_mouse,
                shoot_bow,
                shoot_arrow,
                move_bow_cursor,
                clamp_bow,
                rotate_bow,
                check_arrow_bounds,
                progress_bow,
            )
                .chain(),
        )
        .add_systems(FixedUpdate, despawn_entities)
        .add_systems(FixedUpdate, on_window_change)
        .add_event::<ArrowShotEvent>()
        .add_event::<DespawnEvent>()
        .run();
}

#[derive(Resource, Deref, DerefMut, Reflect)]
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

#[derive(Resource, Deref, DerefMut)]
struct BowArea {
    tl: Vec2,
    br: Vec2,
    #[deref]
    rect: Rect,
}

impl BowArea {
    fn new(tl: Vec2, br: Vec2) -> Self {
        return BowArea {
            tl,
            br,
            rect: Rect::from_corners(tl, br),
        };
    }
}

#[derive(Component)]
struct PullProgressBar;

#[derive(Component, Deref, DerefMut, Default)]
struct BowPullTime(f32);

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

#[derive(Event, Deref, DerefMut)]
struct DespawnEvent(Entity);

#[derive(Component)]
struct Pos(Vec2);

#[derive(Component, Deref, DerefMut)]
struct Vel(Vec2);

#[derive(Component, Deref, DerefMut)]
struct Acc(Vec2);

// Animation
#[derive(Component)]
struct AnimationIndices {
    first: usize,
    last: usize,
}

#[derive(Component, Deref, DerefMut)]
struct AnimationTimer(Timer);

#[derive(Component)]
struct MainCamera;

fn setup(
    window: Query<&Window>,
    mut commands: Commands,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut progress_bar_materials: ResMut<Assets<ProgressBarMaterial>>,
    asset_server: Res<AssetServer>,
) {
    commands.spawn((Camera2dBundle::default(), MainCamera));

    let win = window.single();

    // Bow
    let texture = asset_server.load("bow/bow-atlas.png");
    let size = 190.;
    let layout = TextureAtlasLayout::from_grid(Vec2::new(BOW_SIZE, BOW_SIZE), 3, 3, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);
    // Use only the subset of sprites in the sheet that make up the run animation
    let animation_indices = AnimationIndices { first: 0, last: 7 };
    let bow = commands
        .spawn((
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
            BowPullTime::default(),
            AnimationTimer(Timer::from_seconds(
                BOW_FULL_PULL_TIME / 8.,
                TimerMode::Once,
            )),
            Fixed(false),
        ))
        .id();

    commands.insert_resource(BowArea::new(
        Vec2::new(0., win.height() / 2.),
        Vec2::new(win.width() / -4., win.height() / -2.),
    ));

    let bar = ProgressBar::new(vec![(200, Color::BLUE)]);
    let style = Style {
        position_type: PositionType::Absolute,
        width: Val::Px(BOW_SIZE),
        height: Val::Px(20.),
        ..default()
    };
    let pull_bar = commands
        .spawn((
            PullProgressBar,
            ProgressBarBundle::new(style, bar, &mut progress_bar_materials),
        ))
        .id();
    //commands.entity(bow).add_child(pull_bar);

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

fn on_window_change(window: Query<&Window, Changed<Window>>, mut bow_area: ResMut<BowArea>) {
    for win in &window {
        *bow_area = BowArea::new(
            Vec2::new(win.width() / -2., win.height() / 2.),
            Vec2::new(win.width() / -4., win.height() / -2.),
        )
    }
}

fn draw_bow(
    time: Res<Time>,
    mut query: Query<
        (
            &AnimationIndices,
            &mut BowPullTime,
            &mut AnimationTimer,
            &mut TextureAtlas,
            &Fixed,
        ),
        With<Bow>,
    >,
) {
    // I could probably also do something linke With<Fixed> and then insert the BowPullTime
    // Component later and remove it after the shot
    for (indices, mut pull_time, mut timer, mut atlas, fixed) in &mut query {
        if **fixed {
            timer.tick(time.delta());
            **pull_time += time.delta().as_secs_f32();
            **pull_time = pull_time.clamp(0.0, BOW_FULL_PULL_TIME);
            if timer.just_finished() {
                if atlas.index < indices.last {
                    atlas.index += 1;
                    timer.reset();
                };
            }
        } else {
            atlas.index = indices.first;
            **pull_time = 0.;
            timer.reset();
        }
    }
}

fn progress_bow(
    time: Res<Time>,
    window: Query<&Window>,
    bow_query: Query<(&Fixed, &Transform), With<Bow>>,
    mut progress_query: Query<(&mut ProgressBar, &mut Style), With<PullProgressBar>>,
) {
    let win = window.single();
    let (fixed, bow_transform) = bow_query.single();
    let (mut progress, mut style) = progress_query.single_mut();

    if **fixed {
        // I couldn't get the parent child relationship to work properly for transforms, so
        // I map the "normal" carthesian system into the ui one
        style.top =
            Val::Px((bow_transform.translation.y - win.height() / 2.).abs() + BOW_SIZE / 2.);
        style.left = Val::Px(bow_transform.translation.x + win.width() / 2. - BOW_SIZE / 2.);
        progress.increase_progress(time.delta_seconds() / BOW_FULL_PULL_TIME);
    } else {
        progress.reset();
    }
}

fn draw_bow_area(bow_area: Res<BowArea>, mut gizmos: Gizmos) {
    gizmos.line_2d(
        Vec2::new(bow_area.br.x, bow_area.tl.y),
        Vec2::new(bow_area.br.x, bow_area.br.y),
        Color::DARK_GRAY,
    );
}

fn update_mouse(
    mut mouse: ResMut<Mouse>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
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
    mouse: Res<Mouse>,
    window: Query<&Window>,
    buttons: Res<ButtonInput<MouseButton>>,
    bow: Query<(&Transform, &Fixed, &BowPullTime), With<Bow>>,
    mut shot_event_writer: EventWriter<ArrowShotEvent>,
) {
    let (tr, fixed, pull_time) = bow.single();
    let win = window.single();

    if **fixed && buttons.just_released(MouseButton::Left) {
        // 1 second to reach the window from the left to the right
        let max_vel = win.width();
        let vel = (max_vel / 4.).lerp(max_vel, **pull_time / BOW_FULL_PULL_TIME);

        let dir_to_mouse = (tr.translation - mouse.extend(0.)).normalize();
        let angle = dir_to_mouse.y.atan2(dir_to_mouse.x);

        let vx = vel * angle.cos();
        let vy = vel * angle.sin();

        print!("vx {:?} vy {:?}\n", vx, vy);

        shot_event_writer.send(ArrowShotEvent {
            pos: tr.translation.xy(),
            angle: tr.rotation,
            velocity: Vec2::new(vx, vy),
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

fn clamp_bow(
    bow_area: Res<BowArea>,
    mut bowq: Query<&mut Transform, With<Bow>>,
) {
    let mut bow = bowq.single_mut();
    bow.translation = bow.translation.clamp(Vec3::new(bow_area.tl.x, bow_area.br.y, 0.), Vec3::new(bow_area.br.x, bow_area.tl.y, 1.));
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
    g: Res<G>,
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
            Vel(ev.velocity),
            Acc(Vec2::new(0., -**g)),
        ));
    }
}

fn move_arrows(time: Res<Time>, mut arrows: Query<(&mut Transform, &mut Vel, &Acc), With<Arrow>>) {
    for (mut tr, mut vel, acc) in &mut arrows {
        tr.translation.x += vel.x * time.delta_seconds();
        tr.translation.y += vel.y * time.delta_seconds();
        **vel += **acc;
    }
}

fn check_arrow_bounds(
    arrows: Query<(Entity, &Transform), With<Arrow>>,
    window: Query<&Window>,
    mut despawns: EventWriter<DespawnEvent>,
) {
    let win = window.single();
    for (entity, tr) in &arrows {
        let pos = tr.translation.xy();

        let width = win.width();
        let height = win.height();
        let rect = Rect::from_corners(
            Vec2::new(width / -2., height / 2.),
            Vec2::new(width / 2., height / -2.),
        );

        if !rect.contains(pos) {
            despawns.send(DespawnEvent(entity));
        }
    }
}

fn rotate_arrows(mut arrows: Query<(&mut Transform, &Vel), With<Arrow>>) {
    for (mut tr, vel) in &mut arrows {
        let n = vel.normalize();
        let angle = n.y.atan2(n.x);
        tr.rotation = Quat::from_rotation_z(angle);
    }
}

fn despawn_entities(mut commands: Commands, mut events: EventReader<DespawnEvent>) {
    for ev in events.read() {
        commands.entity(**ev).despawn()
    }
}
