use bevy::prelude::*;
use bevy_garage_car::STATIC_GROUP;
use bevy_rapier3d::prelude::*;
use bevy_renet::renet::{
    transport::NETCODE_KEY_BYTES, ChannelConfig, ClientId, ConnectionConfig, SendType,
};
use serde::{Deserialize, Serialize};
use std::{f32::consts::PI, time::Duration};

pub const PRIVATE_KEY: &[u8; NETCODE_KEY_BYTES] = b"an example very very secret key."; // 32-bytes
pub const PROTOCOL_ID: u64 = 7;

#[derive(Debug, Component)]
pub struct Player {
    pub id: ClientId,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, Component, Resource)]
pub struct PlayerInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

#[derive(Debug, Serialize, Deserialize, Component, Event)]
pub enum PlayerCommand {
    BasicAttack { cast_at: Vec3 },
}

pub enum ClientChannel {
    Input,
    Command,
}

pub enum ServerChannel {
    ServerMessages,
    NetworkedEntities,
}

#[derive(Debug, Serialize, Deserialize, Component)]
pub enum ServerMessages {
    PlayerCreate {
        entity: Entity,
        id: ClientId,
        translation: [f32; 3],
    },
    PlayerRemove {
        id: ClientId,
    },
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct NetworkedEntities {
    pub entities: Vec<Entity>,
    pub translations: Vec<[f32; 3]>,
    pub rotations: Vec<[f32; 4]>,
    pub wheels_translations: Vec<[[f32; 3]; 4]>,
    pub wheels_rotations: Vec<[[f32; 4]; 4]>,
}

impl From<ClientChannel> for u8 {
    fn from(channel_id: ClientChannel) -> Self {
        match channel_id {
            ClientChannel::Command => 0,
            ClientChannel::Input => 1,
        }
    }
}

impl ClientChannel {
    pub fn channels_config() -> Vec<ChannelConfig> {
        vec![
            ChannelConfig {
                channel_id: Self::Input.into(),
                max_memory_usage_bytes: 5 * 1024 * 1024,
                send_type: SendType::ReliableOrdered {
                    resend_time: Duration::ZERO,
                },
            },
            ChannelConfig {
                channel_id: Self::Command.into(),
                max_memory_usage_bytes: 5 * 1024 * 1024,
                send_type: SendType::ReliableOrdered {
                    resend_time: Duration::ZERO,
                },
            },
        ]
    }
}

impl From<ServerChannel> for u8 {
    fn from(channel_id: ServerChannel) -> Self {
        match channel_id {
            ServerChannel::NetworkedEntities => 0,
            ServerChannel::ServerMessages => 1,
        }
    }
}

impl ServerChannel {
    pub fn channels_config() -> Vec<ChannelConfig> {
        vec![
            ChannelConfig {
                channel_id: Self::NetworkedEntities.into(),
                max_memory_usage_bytes: 10 * 1024 * 1024,
                send_type: SendType::Unreliable,
            },
            ChannelConfig {
                channel_id: Self::ServerMessages.into(),
                max_memory_usage_bytes: 10 * 1024 * 1024,
                send_type: SendType::ReliableOrdered {
                    resend_time: Duration::from_millis(200),
                },
            },
        ]
    }
}

pub fn connection_config() -> ConnectionConfig {
    ConnectionConfig {
        available_bytes_per_tick: 1024 * 1024,
        client_channels_config: ClientChannel::channels_config(),
        server_channels_config: ServerChannel::channels_config(),
    }
}

pub fn setup_level(
    mut cmd: Commands,
    #[cfg(feature = "graphics")] mut meshes: ResMut<Assets<Mesh>>,
    #[cfg(feature = "graphics")] mut materials: ResMut<Assets<StandardMaterial>>,
    #[cfg(feature = "graphics")] asset_server: Res<AssetServer>,
) {
    let size = 1000.;
    let size_half = size / 2.;
    let t0 = Vec3::new(0., 0., 0.);
    let cuboid = Collider::cuboid(size_half, 0.5, size_half);
    cmd.spawn((
        cuboid,
        RigidBody::Fixed,
        ColliderScale::Absolute(Vec3::ONE),
        CollisionGroups::new(STATIC_GROUP, Group::ALL),
        Friction::coefficient(3.),
        Restitution::coefficient(0.),
        TransformBundle::from_transform(Transform::from_translation(t0 + Vec3::new(0., -0.5, 0.))),
    ));

    let wall_x = Collider::cuboid(size_half, 10., 1.);
    cmd.spawn((
        wall_x.clone(),
        RigidBody::Fixed,
        ColliderScale::Absolute(Vec3::ONE),
        CollisionGroups::new(STATIC_GROUP, Group::ALL),
        Friction::coefficient(0.5),
        Restitution::coefficient(0.),
        TransformBundle::from_transform(Transform::from_translation(
            t0 + Vec3::new(0., -0.5, -size_half),
        )),
    ));
    cmd.spawn((
        wall_x,
        RigidBody::Fixed,
        ColliderScale::Absolute(Vec3::ONE),
        CollisionGroups::new(STATIC_GROUP, Group::ALL),
        Friction::coefficient(0.5),
        Restitution::coefficient(0.),
        TransformBundle::from_transform(Transform::from_translation(
            t0 + Vec3::new(0., -0.5, size_half),
        )),
    ));
    let wall_z = Collider::cuboid(1., 10., size_half);
    cmd.spawn((
        wall_z.clone(),
        RigidBody::Fixed,
        ColliderScale::Absolute(Vec3::ONE),
        CollisionGroups::new(STATIC_GROUP, Group::ALL),
        Friction::coefficient(0.5),
        Restitution::coefficient(0.),
        TransformBundle::from_transform(Transform::from_translation(
            t0 + Vec3::new(-size_half, -0.5, 0.),
        )),
    ));
    cmd.spawn((
        wall_z,
        RigidBody::Fixed,
        ColliderScale::Absolute(Vec3::ONE),
        CollisionGroups::new(STATIC_GROUP, Group::ALL),
        Friction::coefficient(0.5),
        Restitution::coefficient(0.),
        TransformBundle::from_transform(Transform::from_translation(
            t0 + Vec3::new(size_half, -0.5, 0.),
        )),
    ));

    #[cfg(feature = "graphics")]
    {
        use bevy::render::texture::{
            ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
        };
        // let color_handle = asset_server.load("asphalt/asphalt_color.png");
        // let rough_handle = asset_server.load("asphalt/asphalt_03_rough_4k.exr");
        // let depth_handle = asset_server.load("asphalt/asphalt_03_disp_4k.png");
        let repeat = |settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                address_mode_w: ImageAddressMode::Repeat,
                ..Default::default()
            });
        };
        let normal_handle =
            asset_server.load_with_settings("asphalt/asphalt_03_nor_gl_4k.jpg", repeat);
        let arm_handle = asset_server.load_with_settings("asphalt/asphalt_03_arm_4k.jpg", repeat);
        let diff_handle = asset_server.load_with_settings("asphalt/asphalt_03_diff_4k.jpg", repeat);
        let asphalt_material = materials.add(StandardMaterial {
            base_color: Color::rgb(0.27, 0.25, 0.33),
            normal_map_texture: Some(normal_handle),
            // depth_map: Some(depth_handle),
            metallic: 1.0,
            perceptual_roughness: 1.0,
            metallic_roughness_texture: Some(arm_handle),
            diffuse_transmission_texture: Some(diff_handle),
            ..default()
        });

        let x = 10.;
        let uvs = vec![[x, 0.0], [0.0, 0.0], [0.0, x], [x, x]];
        let mesh = Mesh::from(Rectangle::new(size, size))
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        let rotation = Quat::from_rotation_x(-PI / 2.);
        let transform = Transform::from_translation(t0).with_rotation(rotation);
        cmd.spawn(PbrBundle {
            mesh: meshes.add(mesh),
            material: asphalt_material,
            transform,
            ..Default::default()
        });
        cmd.spawn(DirectionalLightBundle {
            directional_light: DirectionalLight {
                shadows_enabled: true,
                ..default()
            },
            transform: Transform {
                translation: Vec3::new(0.0, 2.0, 0.0),
                rotation: Quat::from_rotation_x(-PI / 4.),
                ..default()
            },
            ..default()
        });
    }
}
