use bevy::{
    app::{App, FixedUpdate, Startup, Update},
    asset::{AssetServer, Assets},
    ecs::reflect,
    hierarchy::BuildChildren,
    input::ButtonInput,
    math::{FloatExt, Quat, Rect, Vec2, Vec2Swizzles, Vec3, Vec3Swizzles},
    prelude::{
        default, Camera2dBundle, Changed, Circle, Commands, Component, Deref, DerefMut, Entity,
        Event, EventReader, EventWriter, Gizmos, IntoSystemConfigs, Line2d, MouseButton, Query,
        ReflectResource, Res, ResMut, Resource, With,
    },
    reflect::{FromReflect, Reflect},
    render::{
        camera::Camera, color::Color, mesh::Mesh, render_resource::encase::rts_array::Length,
    },
    sprite::{
        ColorMaterial, MaterialMesh2dBundle, Sprite, SpriteBundle, SpriteSheetBundle, TextureAtlas,
        TextureAtlasLayout,
    },
    text::{Text, TextSection, TextStyle},
    time::{Time, Timer, TimerMode},
    transform::components::{GlobalTransform, Transform},
    ui::{node_bundles::TextBundle, PositionType, Style, Val},
    window::{PrimaryWindow, Window},
    DefaultPlugins,
};
use bevy_bow::{ProgressBar, ProgressBarBundle, ProgressBarMaterial, ProgressBarPlugin};
use bevy_editor_pls::EditorPlugin;
use bevy_inspector_egui::quick::{ResourceInspectorPlugin, WorldInspectorPlugin};
use rand::prelude::*;

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
        .insert_resource(Scoreboard(0))
        .insert_resource(G(18.))
        .insert_resource(SpawnTimer(Timer::from_seconds(2., TimerMode::Repeating)))
        .add_plugins(ResourceInspectorPlugin::<G>::new())
        .add_plugins(ResourceInspectorPlugin::<SpawnTimer>::new())
        .add_systems(Startup, (setup).chain())
        .add_systems(
            Update,
            (
                animate_bow,
                draw_bow_area,
                draw_enemy_area,
                move_arrows,
                rotate_arrows,
                move_enemy,
            )
                .chain(),
        )
        .add_systems(
            FixedUpdate,
            (
                update_mouse,
                shoot_bow,
                shoot_arrow,
                check_arrow_collision,
                move_bow_cursor,
                update_scoreboard,
                clamp_bow,
                rotate_bow,
                check_arrow_bounds,
                progress_bow,
                spawn_enemy,
                animate_enemy,
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

#[derive(Resource, Deref, DerefMut)]
struct Scoreboard(u32);

#[derive(Component)]
struct ScoreboardUi;

#[derive(Component)]
struct Bow;

#[derive(Resource, Deref, DerefMut)]
struct BowArea(Area);

#[derive(Resource, Deref, DerefMut)]
struct EnemyArea(Area);

#[derive(Clone, Copy, PartialEq)]
enum Side {
    North,
    East,
    South,
    West,
}

#[derive(Component, Deref)]
struct EndsAt(Side);

struct Area {
    tl: Vec2,
    br: Vec2,
    rect: Rect,
    walls: [(Side, Vec2, Vec2); 4],
}

impl Area {
    fn new(tl: Vec2, br: Vec2) -> Self {
        let walls = [
            (Side::East, Vec2::new(tl.x, tl.y), Vec2::new(br.x, tl.y)),
            (Side::South, Vec2::new(br.x, tl.y), Vec2::new(br.x, br.y)),
            (Side::West, Vec2::new(br.x, br.y), Vec2::new(tl.x, br.y)),
            (Side::North, Vec2::new(tl.x, br.y), Vec2::new(tl.x, tl.y)),
        ];

        return Area {
            tl,
            br,
            rect: Rect::from_corners(tl, br),
            walls,
        };
    }

    fn shrink(self, factor: f32) -> Self {
        let tl = self.tl.lerp(self.br, factor);
        let br = self.br.lerp(self.tl, factor);
        return Area::new(tl, br);
    }
}

#[derive(Component)]
struct Path {
    start: Vec2,
    end: Vec2,
}

impl Path {
    fn reverse(mut self) {
        std::mem::swap(&mut self.start, &mut self.end);
    }
}

trait PathFindingStrategy {
    fn find(self, line1: &(Side, Vec2, Vec2), line2: &(Side, Vec2, Vec2)) -> Path;
    fn find_from_start(self, line1: &Vec2, line2: &(Side, Vec2, Vec2)) -> Path;
}

#[derive(Resource, Copy, Clone)]
struct MinLengthPathFinder(f32);

impl PathFindingStrategy for MinLengthPathFinder {
    fn find(self, line1: &(Side, Vec2, Vec2), line2: &(Side, Vec2, Vec2)) -> Path {
        let min = self.0;
        let mut start: Vec2;
        let mut end: Vec2;
        loop {
            start = random_point_on_line(line1.1, line1.2);
            end = random_point_on_line(line2.1, line2.2);
            if (end - start).length() >= min {
                break;
            }
        }
        return Path { start, end };
    }

    fn find_from_start(self, line1: &Vec2, line2: &(Side, Vec2, Vec2)) -> Path {
        let min = self.0;
        let mut start: Vec2;
        let mut end: Vec2;
        loop {
            start = *line1;
            end = random_point_on_line(line2.1, line2.2);
            if (end - start).length() >= min {
                break;
            }
        }
        return Path { start, end };
    }
}

fn random_point_on_line(from: Vec2, to: Vec2) -> Vec2 {
    let t = rand::random::<f32>();
    return from.lerp(to, t);
}

fn pick<T: Clone>(amount: usize, slice: &Vec<T>) -> Vec<usize> {
    let mut rng = rand::thread_rng();

    rand::seq::index::sample(&mut rng, slice.length(), amount).into_vec()
}

#[derive(Resource, Deref, DerefMut, Reflect)]
struct SpawnTimer(Timer);

#[derive(Component)]
struct Enemy;

fn spawn_enemy(
    mut commands: Commands,
    time: Res<Time>,
    path_finder: Res<MinLengthPathFinder>,
    mut timer: ResMut<SpawnTimer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    asset_server: Res<AssetServer>,
    enemy_area: Res<EnemyArea>,
) {
    timer.tick(time.delta());
    if timer.just_finished() {
        let texture = asset_server.load("enemy/enemy.png");
        let layout = TextureAtlasLayout::from_grid(Vec2::new(500. / 8., 50.), 8, 1, None, None);
        let texture_atlas_layout = texture_atlas_layouts.add(layout);

        let animation_indices = AnimationIndices { first: 0, last: 7 };

        let walls = enemy_area.walls.to_vec();
        let is = &pick(2, &walls)[..];
        let start_wall = &walls[is[0]];
        let end_wall = &walls[is[1]];

        let path = path_finder.find(start_wall, end_wall);
        let size = thread_rng().gen_range((500. / 8.)..100.);

        commands.spawn((
            SpriteSheetBundle {
                texture,
                atlas: TextureAtlas {
                    layout: texture_atlas_layout,
                    index: animation_indices.first,
                },
                transform: Transform {
                    translation: path.start.extend(0.),
                    ..default()
                },
                sprite: Sprite {
                    custom_size: Some(Vec2::new(size, size)),
                    ..default()
                },
                ..default()
            },
            Speed(200.),
            AnimationTimer(Timer::from_seconds(0.2, TimerMode::Repeating)),
            animation_indices,
            path,
            EndsAt(end_wall.0.clone()),
            Enemy,
        ));
    }
}

fn animate_enemy(
    time: Res<Time>,
    mut query: Query<
        (
            &AnimationIndices,
            &mut Sprite,
            &mut AnimationTimer,
            &mut TextureAtlas,
            &Path,
        ),
        With<Enemy>,
    >,
) {
    for (indices, mut sprite, mut timer, mut atlas, path) in &mut query {
        // sprite looks to the left by default
        sprite.flip_x = (path.end - path.start).x > 0.;

        timer.tick(time.delta());
        if timer.just_finished() {
            if atlas.index < indices.last {
                atlas.index += 1;
            } else {
                atlas.index = indices.first;
            }
        }
    }
}

fn move_enemy(
    time: Res<Time>,
    path_finder: Res<MinLengthPathFinder>,
    enemy_area: Res<EnemyArea>,
    mut query: Query<(&Speed, &mut Transform, &mut Path, &EndsAt), With<Enemy>>,
) {
    for (speed, mut transform, mut path, EndsAt(side)) in &mut query {
        let vel = (path.end - path.start).normalize() * **speed * time.delta_seconds();
        transform.translation += vel.extend(0.);
        let px_threshold = 5.;
        if (transform.translation - path.end.extend(0.)).length() < px_threshold {
            let start = path.end;
            let walls: Vec<&(Side, Vec2, Vec2)> = enemy_area
                .walls
                .iter()
                .filter(|wall| wall.0 != *side)
                .collect();
            let index = pick(1, &walls)[..][0];
            let end_wall = walls[..][index];
            *path = path_finder.find_from_start(&start, end_wall);
        }
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

#[derive(Component, Deref, DerefMut)]
struct Vel(Vec2);

#[derive(Component, Deref, DerefMut)]
struct Speed(f32);

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
    let layout = TextureAtlasLayout::from_grid(Vec2::new(BOW_SIZE, BOW_SIZE), 3, 3, None, None);
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
        BowPullTime::default(),
        AnimationTimer(Timer::from_seconds(
            BOW_FULL_PULL_TIME / 8.,
            TimerMode::Once,
        )),
        Fixed(false),
    ));

    commands.insert_resource(BowArea(Area::new(
        Vec2::new(0., win.height() / 2.),
        Vec2::new(win.width() / -4., win.height() / -2.),
    )));

    let enemy_area = EnemyArea(
        Area::new(
            Vec2::new(0., win.height() / 2.),
            Vec2::new(win.width() / 2., win.height() / -2.),
        )
        .shrink(0.1),
    );
    let path_finder = MinLengthPathFinder(enemy_area.rect.width() / 2.);
    commands.insert_resource(enemy_area);
    commands.insert_resource(path_finder);

    let bar = ProgressBar::new(vec![(200, Color::BLUE)]);
    let style = Style {
        position_type: PositionType::Absolute,
        width: Val::Px(BOW_SIZE),
        height: Val::Px(20.),
        ..default()
    };
    commands.spawn((
        PullProgressBar,
        ProgressBarBundle::new(style, bar, &mut progress_bar_materials),
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

fn on_window_change(
    window: Query<&Window, Changed<Window>>,
    mut bow_area: ResMut<BowArea>,
    mut enemy_area: ResMut<EnemyArea>,
) {
    for win in &window {
        *bow_area = BowArea(Area::new(
            Vec2::new(win.width() / -2., win.height() / 2.),
            Vec2::new(win.width() / -4., win.height() / -2.),
        ));
        *enemy_area = EnemyArea(
            Area::new(
                Vec2::new(0., win.height() / 2.),
                Vec2::new(win.width() / 2., win.height() / -2.),
            )
            .shrink(0.1),
        )
    }
}

fn animate_bow(
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
    // I could probably also do something like With<Fixed> and then insert the BowPullTime
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
    for marker in &bow_area.walls {
        gizmos.line_2d(marker.1, marker.2, Color::DARK_GRAY);
    }
}

fn draw_enemy_area(enemy_area: Res<EnemyArea>, mut gizmos: Gizmos) {
    for marker in &enemy_area.walls {
        gizmos.line_2d(marker.1, marker.2, Color::DARK_GRAY);
    }
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

fn clamp_bow(bow_area: Res<BowArea>, mut bowq: Query<&mut Transform, With<Bow>>) {
    let mut bow = bowq.single_mut();
    bow.translation = bow.translation.clamp(
        Vec3::new(bow_area.tl.x, bow_area.br.y, 0.),
        Vec3::new(bow_area.br.x, bow_area.tl.y, 1.),
    );
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

fn check_arrow_collision(
    mut score: ResMut<Scoreboard>,
    arrows: Query<(Entity, &Transform), With<Arrow>>,
    enemies: Query<(Entity, &Transform, &Sprite), With<Enemy>>,
    mut despawns: EventWriter<DespawnEvent>,
) {
    for (arrow_entity, arrow_tr) in &arrows {
        for (enemy_entity, enemy_tr, enemy_size) in &enemies {
            if (arrow_tr.translation.xy() - enemy_tr.translation.xy()).length()
                < enemy_size.custom_size.unwrap().x
            {
                despawns.send(DespawnEvent(arrow_entity));
                despawns.send(DespawnEvent(enemy_entity));
                **score += 1
            }
        }
    }
}

fn update_scoreboard(
    score: Res<Scoreboard>,
    mut query: Query<&mut Text, With<ScoreboardUi>>,
) {
    let mut text = query.single_mut();
    text.sections[1].value = (**score).to_string();
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
