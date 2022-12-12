use bevy::{
    core_pipeline::{bloom::BloomSettings, fxaa::Fxaa},
    prelude::*,
};
use bevy_obj::*;
use bevy_voxel_engine::{
    BevyVoxelEnginePlugin, Box, BoxCollider, Edges, Particle, Portal, Velocity, VoxelCameraBundle,
    VoxelizationBundle, VoxelizationMaterial, VOXELS_PER_METER, LoadVoxelWorld,
};
use character::CharacterEntity;
use concurrent_queue::ConcurrentQueue;
use std::f32::consts::PI;

mod character;
mod fps_counter;
mod ui;

// zero: normal bullet
// one: orange portal bullet
// two: blue portal bullet
#[derive(Component)]
pub struct Bullet {
    bullet_type: u32,
}

#[derive(Resource)]
pub struct Settings {
    pub spectator: bool,
}

fn main() {
    let mut app = App::new();
    app.insert_resource(Settings { spectator: false })
        .add_plugins(DefaultPlugins)
        .add_plugin(ObjPlugin)
        .add_plugin(BevyVoxelEnginePlugin)
        .add_plugin(character::Character)
        .add_plugin(ui::UiPlugin)
        .add_plugin(fps_counter::FpsCounter)
        .add_startup_system(setup)
        .add_system(update)
        .add_system(shoot)
        .add_system(update_velocitys)
        .add_system(spawn_portals);

    let dot = bevy_mod_debugdump::get_render_graph(&mut app);
    std::fs::write("render-graph.dot", dot).expect("Failed to write render-graph.dot");
    println!("Render graph written to render-graph.dot");

    app.run();
}

// world space cordinates are in terms of 4 voxels per meter with 0, 0
// in the world lining up with the center of the voxel world and the edge
// of the world being half of the world size in each direction
fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut load_voxel_world: ResMut<LoadVoxelWorld>,
) {
    *load_voxel_world = LoadVoxelWorld::File("assets/monu9.vox".to_string());

    let portal1 = commands
        .spawn((
            Portal {
                half_size: IVec3::new(0, 0, 0),
                normal: Vec3::new(1.0, 0.0, 0.0),
            },
            Edges {
                material: 120,
                half_size: IVec3::new(0, 0, 0),
            },
            Transform::from_xyz(0.0, 1000.0, 0.0),
        ))
        .id();
    let portal2 = commands
        .spawn((
            Portal {
                half_size: IVec3::new(0, 0, 0),
                normal: Vec3::new(1.0, 0.0, 0.0),
            },
            Edges {
                material: 121,
                half_size: IVec3::new(0, 0, 0),
            },
            Transform::from_xyz(0.0, 1000.0, 0.0),
        ))
        .id();

    // character
    let character_transform = Transform::from_xyz(10.0, 10.0, -5.0).looking_at(Vec3::ZERO, Vec3::Y);
    let projection = Projection::Perspective(PerspectiveProjection {
        fov: PI / 2.0,
        ..default()
    });
    commands
        .spawn((
            VoxelCameraBundle {
                transform: character_transform,
                projection: projection.clone(),
                ..default()
            },
            CharacterEntity {
                grounded: false,
                look_at: -character_transform.local_z(),
                up: Vec3::new(0.0, 1.0, 0.0),
                portal1,
                portal2,
            },
            Velocity::new(Vec3::splat(0.0)),
            BoxCollider {
                half_size: IVec3::new(2, 4, 2),
            },
            BloomSettings::default(),
            Fxaa::default(),
        ))
        .with_children(|parent| {
            // voxelization preview camera
            parent.spawn((
                Camera3dBundle {
                    camera: Camera {
                        is_active: false,
                        priority: 10, // render after the main camera
                        ..default()
                    },
                    projection,
                    ..default()
                },
                BloomSettings::default(),
                Fxaa::default(),
                VoxelizationPreviewCamera,
            ));
        });

    // voxelized mesh
    commands.spawn((
        VoxelizationBundle {
            mesh_handle: asset_server.load("models/suzanne.obj"),
            voxelization_material: VoxelizationMaterial {
                texture: asset_server.load("models/suzanne.png"),
            },
            transform: Transform::from_scale(Vec3::splat(5.0)).looking_at(Vec3::Z, Vec3::Y),
            ..default()
        },
        Suzanne,
    ));
}

#[derive(Component)]
struct VoxelizationPreviewCamera;
#[derive(Component)]
struct Suzanne;

fn update(time: Res<Time>, mut cube: Query<&mut Transform, With<Suzanne>>) {
    for mut transform in cube.iter_mut() {
        transform.rotate_x(1.5 * time.delta_seconds());
        transform.rotate_z(1.3 * time.delta_seconds());
    }
}

fn shoot(
    mut commands: Commands,
    input: Res<Input<MouseButton>>,
    keyboard: Res<Input<KeyCode>>,
    character: Query<&Transform, With<CharacterEntity>>,
    mut settings: ResMut<Settings>,
) {
    let character = character.single();

    if input.just_pressed(MouseButton::Left) {
        commands.spawn((
            Transform::from_translation(character.translation),
            Particle { material: 120 },
            Velocity::new(-character.local_z() * 50.0),
            Bullet { bullet_type: 1 },
        ));
    }
    if input.just_pressed(MouseButton::Right) {
        commands.spawn((
            Transform::from_translation(character.translation),
            Particle { material: 121 },
            Velocity::new(-character.local_z() * 50.0),
            Bullet { bullet_type: 2 },
        ));
    }

    if keyboard.just_pressed(KeyCode::P) {
        settings.spectator = !settings.spectator;
    }

    if keyboard.just_pressed(KeyCode::B) {
        commands.spawn((
            Transform::from_translation(character.translation),
            Velocity::new(-character.local_z() * 10.0),
            Bullet { bullet_type: 0 },
            BoxCollider {
                half_size: IVec3::new(3, 3, 3),
            },
            Box {
                material: 14,
                half_size: IVec3::new(3, 3, 3),
            },
        ));
    }
}

fn update_velocitys(
    mut commands: Commands,
    mut velocity_query: Query<(&Transform, &mut Velocity, Entity), With<Bullet>>,
    time: Res<Time>,
) {
    let to_destroy = ConcurrentQueue::unbounded();
    velocity_query.par_for_each_mut(8, |(_transform, mut velocity, _entity)| {
        velocity.velocity += Vec3::new(0.0, -9.81 * time.delta_seconds(), 0.0);
        // let e = animation::world_to_render(transform.translation.abs(), uniforms.texture_size);
        // if e.x > 1.0 || e.y > 1.0 || e.z > 1.0 {
        //     to_destroy.push(entity).unwrap();
        // }
    });

    while let Ok(entity) = to_destroy.pop() {
        commands.entity(entity).despawn();
    }
}

fn spawn_portals(
    mut commands: Commands,
    bullet_query: Query<(&Transform, &Velocity, &Bullet, Entity)>,
    mut character_query: Query<&mut CharacterEntity>,
) {
    for (transform, velocity, bullet, entity) in bullet_query.iter() {
        if bullet.bullet_type == 1 || bullet.bullet_type == 2 {
            if velocity.hit_normal != Vec3::splat(0.0) {
                commands.entity(entity).despawn();

                let normal = velocity.hit_normal;
                let pos = ((transform.translation + normal * (0.5 / VOXELS_PER_METER))
                    * VOXELS_PER_METER)
                    .floor()
                    / VOXELS_PER_METER;

                let plane = (Vec3::splat(1.0) - normal.abs()).as_ivec3();

                let mut character = character_query.single_mut();
                if bullet.bullet_type == 1 {
                    commands.entity(character.portal1).despawn();
                    character.portal1 = commands
                        .spawn((
                            Portal {
                                half_size: plane * 5,
                                normal: normal,
                            },
                            Edges {
                                material: 120,
                                half_size: plane * 6,
                            },
                            Transform::from_xyz(pos.x, pos.y, pos.z),
                        ))
                        .id();
                }
                if bullet.bullet_type == 2 {
                    commands.entity(character.portal2).despawn();
                    character.portal2 = commands
                        .spawn((
                            Portal {
                                half_size: plane * 5,
                                normal: normal,
                            },
                            Edges {
                                material: 121,
                                half_size: plane * 6,
                            },
                            Transform::from_xyz(pos.x, pos.y, pos.z),
                        ))
                        .id();
                }
            }
        }
    }
}

// fn spawn_world_portals() {
// commands.spawn((
//     Portal {
//         half_size: IVec3::new(0, 9, 6),
//         normal: Vec3::new(1.0, 0.0, 0.0),
//     },
//     Edges {
//         material: 23,
//         half_size: IVec3::new(0, 10, 7),
//     },
//     Transform::from_xyz(3.0, 2.0, 0.0),
// ));
// commands.spawn((
//     Portal {
//         half_size: IVec3::new(6, 9, 0),
//         normal: Vec3::new(0.0, 0.0, 1.0),
//     },
//     Edges {
//         material: 22,
//         half_size: IVec3::new(7, 10, 0),
//     },
//     Transform::from_xyz(0.0, 2.0, 3.0),
// ));

// commands.spawn((
//     Portal {
//         half_size: IVec3::new(0, 1, 1),
//         normal: Vec3::new(1.0, 0.0, 0.0),
//     },
//     Edges {
//         material: 23,
//         half_size: IVec3::new(0, 2, 2),
//     },
//     Transform::from_xyz(3.0, 5.0, 0.0),
// ));
// commands.spawn((
//     Portal {
//         half_size: IVec3::new(1, 1, 0),
//         normal: Vec3::new(0.0, 0.0, 1.0),
//     },
//     Edges {
//         material: 22,
//         half_size: IVec3::new(2, 2, 0),
//     },
//     Transform::from_xyz(0.0, 5.0, 3.0),
// ));

// commands.spawn((
//     Portal {
//         half_size: IVec3::new(5, 0, 5),
//         normal: Vec3::new(0.0, 1.0, 0.0),
//     },
//     Edges {
//         material: 22,
//         half_size: IVec3::new(6, 0, 6),
//     },
//     Transform::from_xyz(0.0, -1.0, 0.0),
// ));
// commands.spawn((
//     Portal {
//         half_size: IVec3::new(5, 0, 5),
//         normal: Vec3::new(0.0, -1.0, 0.0),
//     },
//     Edges {
//         material: 22,
//         half_size: IVec3::new(6, 0, 6),
//     },
//     Transform::from_xyz(0.0, 7.0, 0.0),
// ));
// }