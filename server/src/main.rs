use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
};
use bevy_garage_arena_lib::{
    connection_config, setup_level, ClientChannel, NetworkedEntities, Player, PlayerCommand,
    PlayerInput, ServerChannel, ServerMessages, PROTOCOL_ID,
};
use bevy_garage_car::{esp_system, spawn_car, Car, CarWheels, Wheel};
use bevy_rapier3d::prelude::*;
use bevy_renet::{
    renet::{
        transport::{NetcodeServerTransport, ServerAuthentication, ServerConfig},
        ClientId, RenetServer, ServerEvent,
    },
    transport::NetcodeServerPlugin,
    RenetServerPlugin,
};
use std::{
    collections::HashMap,
    net::UdpSocket,
    time::{Duration, SystemTime},
};

#[derive(Debug, Default, Resource)]
pub struct ServerLobby {
    pub players: HashMap<ClientId, Entity>,
}

fn new_renet_server() -> (RenetServer, NetcodeServerTransport) {
    let server = RenetServer::new(connection_config());

    let addr = if let Ok(addr) = std::env::var("RENET_SERVER_SOCKET") {
        addr
    } else {
        let default = "127.0.0.1:5000".to_string();
        println!("RENET_SERVER_SOCKET not set, setting default: {}", &default);
        default
    };

    let public_addr = addr.parse().unwrap();
    println!("socket binding to {}", public_addr);
    let socket = UdpSocket::bind(public_addr).unwrap();
    println!("socket bind");
    let current_time: Duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    let server_config = ServerConfig {
        current_time,
        max_clients: 64,
        protocol_id: PROTOCOL_ID,
        public_addresses: vec![public_addr],
        authentication: ServerAuthentication::Unsecure,
    };

    let transport = NetcodeServerTransport::new(server_config, socket).unwrap();

    (server, transport)
}

fn main() {
    let mut app = App::new();
    #[cfg(feature = "graphics")]
    {
        app.insert_resource(bevy_garage_car::CarRes {
            show_rays: true,
            ..default()
        });
        app.add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Bevy Garage renet server".to_string(),
                    resolution: bevy::window::WindowResolution::new(640., 240.),
                    canvas: Some("#bevy-garage".to_string()),
                    ..default()
                }),
                ..default()
            }),
            RapierDebugRenderPlugin::default(),
            bevy_egui::EguiPlugin,
        ));
        app.add_systems(
            Startup,
            (bevy_garage_car::car_start_system, setup_simple_camera),
        );
        app.add_systems(Update, (update_visulizer_system,));
        app.insert_resource(renet_visualizer::RenetServerVisualizer::<200>::default());
    }
    #[cfg(not(feature = "graphics"))]
    app.add_plugins(
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(
            Duration::from_secs_f64(1. / 60.),
        )),
    );

    app.add_plugins((
        RenetServerPlugin,
        NetcodeServerPlugin,
        RapierPhysicsPlugin::<NoUserData>::default(),
        FrameTimeDiagnosticsPlugin,
        LogDiagnosticsPlugin::default(),
    ));

    app.insert_resource(RapierConfiguration {
        timestep_mode: TimestepMode::Variable {
            max_dt: 1. / 60.,
            time_scale: 1.,
            substeps: 5,
        },
        ..default()
    });
    app.insert_resource(ServerLobby::default());

    let (server, transport) = new_renet_server();
    app.insert_resource(server).insert_resource(transport);

    app.add_systems(
        Update,
        (
            server_update_system,
            server_network_sync,
            move_players_system,
            esp_system.after(move_players_system),
        ),
    );
    app.add_systems(Startup, (rapier_config_start_system, setup_level));
    println!("before app run");
    app.run();
    println!("after app run");
}
fn rapier_config_start_system(mut c: ResMut<RapierContext>) {
    c.integration_parameters.num_solver_iterations = std::num::NonZeroUsize::new(4).unwrap();
    c.integration_parameters.num_internal_pgs_iterations = 48;
    c.integration_parameters.num_additional_friction_iterations = 4;
    c.integration_parameters.erp = 0.99;
    // c.integration_parameters.joint_erp = 0.95;
    dbg!(c.integration_parameters);
}

#[allow(clippy::too_many_arguments)]
fn server_update_system(
    mut server_events: EventReader<ServerEvent>,
    mut cmd: Commands,
    mut lobby: ResMut<ServerLobby>,
    mut server: ResMut<RenetServer>,
    players: Query<(Entity, &Player, &Transform)>,
    #[cfg(feature = "graphics")] car_res: Res<bevy_garage_car::CarRes>,
    #[cfg(feature = "graphics")] mut visualizer: ResMut<
        renet_visualizer::RenetServerVisualizer<200>,
    >,
) {
    for event in server_events.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                println!("Player {} connected.", client_id);
                #[cfg(feature = "graphics")]
                visualizer.add_client(client_id.to_owned());

                for (entity, player, transform) in players.iter() {
                    let translation: [f32; 3] = transform.translation.into();
                    let message = bincode::serialize(&ServerMessages::PlayerCreate {
                        id: player.id,
                        entity,
                        translation,
                    })
                    .unwrap();
                    server.send_message(
                        client_id.to_owned(),
                        ServerChannel::ServerMessages,
                        message,
                    );
                }
                let transform = Transform::from_xyz(
                    (fastrand::f32() - 0.5) * 40.,
                    1.51,
                    (fastrand::f32() - 0.5) * 40.,
                );
                let player_entity = spawn_car(
                    &mut cmd,
                    #[cfg(feature = "graphics")]
                    &car_res.car_scene.as_ref().unwrap(),
                    #[cfg(feature = "graphics")]
                    &car_res.wheel_scene.as_ref().unwrap(),
                    false,
                    transform,
                );
                cmd.entity(player_entity)
                    .insert(Player {
                        id: client_id.to_owned(),
                    })
                    .insert(PlayerInput::default());

                lobby.players.insert(client_id.to_owned(), player_entity);

                let translation: [f32; 3] = transform.translation.into();
                let message = bincode::serialize(&ServerMessages::PlayerCreate {
                    id: client_id.to_owned(),
                    entity: player_entity,
                    translation,
                })
                .unwrap();
                server.broadcast_message(ServerChannel::ServerMessages, message);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                println!("Player {} disconnected: {}", client_id, reason);
                #[cfg(feature = "graphics")]
                visualizer.remove_client(client_id.to_owned());
                if let Some(player_entity) = lobby.players.remove(&client_id) {
                    cmd.entity(player_entity).despawn();
                }

                let message = bincode::serialize(&ServerMessages::PlayerRemove {
                    id: client_id.to_owned(),
                })
                .unwrap();
                server.broadcast_message(ServerChannel::ServerMessages, message);
            }
        }
    }

    for client_id in server.clients_id() {
        while let Some(message) = server.receive_message(client_id, ClientChannel::Command) {
            let command: PlayerCommand = bincode::deserialize(&message).unwrap();
            match command {
                PlayerCommand::BasicAttack { cast_at } => {
                    println!(
                        "Received basic attack from client {}: {:?}",
                        client_id, cast_at
                    );
                }
            }
        }
        while let Some(message) = server.receive_message(client_id, ClientChannel::Input) {
            let input: PlayerInput = bincode::deserialize(&message).unwrap();
            if let Some(player_entity) = lobby.players.get(&client_id) {
                cmd.entity(*player_entity).insert(input);
            }
        }
    }
}

#[cfg(feature = "graphics")]
fn update_visulizer_system(
    mut egui_contexts: bevy_egui::EguiContexts,
    mut visualizer: ResMut<renet_visualizer::RenetServerVisualizer<200>>,
    server: Res<RenetServer>,
) {
    visualizer.update(&server);
    visualizer.show_window(egui_contexts.ctx_mut());
}

#[allow(clippy::type_complexity)]
fn server_network_sync(
    mut server: ResMut<RenetServer>,
    mut tr_set: ParamSet<(
        Query<(Entity, &Transform, &CarWheels), With<Player>>,
        Query<&Transform, With<Wheel>>,
    )>,
) {
    let mut networked_entities = NetworkedEntities::default();
    let mut wheels_all: Vec<[Entity; 4]> = vec![];
    for (entity, transform, wheels) in tr_set.p0().iter() {
        networked_entities.entities.push(entity);
        networked_entities
            .translations
            .push(transform.translation.into());
        networked_entities.rotations.push(transform.rotation.into());

        wheels_all.push(wheels.entities);
    }

    for wheels in wheels_all {
        networked_entities.wheels_translations.push([
            tr_set.p1().get(wheels[0]).unwrap().translation.into(),
            tr_set.p1().get(wheels[1]).unwrap().translation.into(),
            tr_set.p1().get(wheels[2]).unwrap().translation.into(),
            tr_set.p1().get(wheels[3]).unwrap().translation.into(),
        ]);
        networked_entities.wheels_rotations.push([
            tr_set.p1().get(wheels[0]).unwrap().rotation.into(),
            tr_set.p1().get(wheels[1]).unwrap().rotation.into(),
            tr_set.p1().get(wheels[2]).unwrap().rotation.into(),
            tr_set.p1().get(wheels[3]).unwrap().rotation.into(),
        ]);
    }

    let sync_message = bincode::serialize(&networked_entities).unwrap();
    server.broadcast_message(ServerChannel::NetworkedEntities, sync_message);
}

fn move_players_system(mut query: Query<(&PlayerInput, &mut Car, &Transform)>) {
    for (input, mut car, t) in query.iter_mut() {
        dbg!(t.translation);
        if input.up {
            car.gas = 1.;
        } else {
            car.gas = 0.;
        }
        if input.down {
            car.brake = 1.;
        } else {
            car.brake = 0.;
        }
        if input.left {
            car.steering = -1.;
        }
        if input.right {
            car.steering = 1.;
        }
        if !input.left && !input.right {
            car.steering = 0.;
        }
    }
}

#[cfg(feature = "graphics")]
pub fn setup_simple_camera(mut commands: Commands) {
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(-20.5, 30.0, 20.5).looking_at(Vec3::ZERO, Vec3::Y),
        ..Default::default()
    });
}
