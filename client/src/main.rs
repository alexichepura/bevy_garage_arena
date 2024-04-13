use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
};
use bevy_egui::{EguiContexts, EguiPlugin};
use bevy_garage_arena_lib::{
    connection_config, setup_level, ClientChannel, NetworkedEntities, PlayerCommand, PlayerInput,
    ServerChannel, ServerMessages, PROTOCOL_ID,
};
use bevy_garage_camera::CarCameraPlugin;
use bevy_garage_car::{CarWheels, Wheel};
use bevy_renet::{
    client_connected,
    renet::{
        transport::{ClientAuthentication, NetcodeClientTransport, NetcodeTransportError},
        ClientId, RenetClient,
    },
    transport::NetcodeClientPlugin,
    RenetClientPlugin,
};
use renet_visualizer::{RenetClientVisualizer, RenetVisualizerStyle};
use std::{collections::HashMap, net::UdpSocket, time::SystemTime};

#[derive(Component)]
struct ControlledPlayer;

#[derive(Default, Resource)]
struct NetworkMapping(HashMap<Entity, Entity>);

#[derive(Debug)]
struct PlayerInfo {
    client_entity: Entity,
    server_entity: Entity,
}

#[derive(Debug, Default, Resource)]
struct ClientLobby {
    players: HashMap<ClientId, PlayerInfo>,
}

fn new_renet_client() -> (RenetClient, NetcodeClientTransport) {
    let client = RenetClient::new(connection_config());

    let addr = if let Ok(addr) = std::env::var("RENET_SERVER_ADDR") {
        println!("RENET_SERVER_ADDR: {}", &addr);
        addr
    } else {
        let default = "127.0.0.1:5000".to_string();
        println!("RENET_SERVER_ADDR not set, setting default: {}", &default);
        default
    };

    let server_addr = addr.parse().unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let current_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    let client_id = current_time.as_millis() as u64;
    let authentication = ClientAuthentication::Unsecure {
        client_id,
        protocol_id: PROTOCOL_ID,
        server_addr,
        user_data: None,
    };

    let transport = NetcodeClientTransport::new(current_time, authentication, socket).unwrap();

    (client, transport)
}

fn main() {
    let mut app = App::new();
    app.insert_resource(bevy_garage_car::CarRes {
        show_rays: true,
        ..default()
    });
    app.add_plugins((
        DefaultPlugins,
        RenetClientPlugin,
        NetcodeClientPlugin,
        FrameTimeDiagnosticsPlugin,
        LogDiagnosticsPlugin::default(),
        EguiPlugin,
        CarCameraPlugin,
    ));
    app.add_event::<PlayerCommand>();
    app.insert_resource(ClientLobby::default());
    app.insert_resource(PlayerInput::default());
    let (client, transport) = new_renet_client();
    app.insert_resource(client);
    app.insert_resource(transport);

    app.insert_resource(NetworkMapping::default());

    app.add_systems(
        Update,
        (
            player_input,
            (
                client_send_input,
                client_send_player_commands,
                client_sync_players,
            )
                .run_if(client_connected),
        ),
    );

    app.insert_resource(RenetClientVisualizer::<200>::new(
        RenetVisualizerStyle::default(),
    ));

    app.add_systems(Startup, (setup_level, bevy_garage_car::car_start_system));
    app.add_systems(Update, (update_visulizer_system, panic_on_error_system));

    app.run();
}

// If any error is found we just panic
fn panic_on_error_system(mut renet_error: EventReader<NetcodeTransportError>) {
    for e in renet_error.read() {
        panic!("{}", e);
    }
}

fn update_visulizer_system(
    mut egui_contexts: EguiContexts,
    mut visualizer: ResMut<RenetClientVisualizer<200>>,
    client: Res<RenetClient>,
    mut show_visualizer: Local<bool>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    visualizer.add_network_info(client.network_info());
    if keyboard_input.just_pressed(KeyCode::F1) {
        *show_visualizer = !*show_visualizer;
    }
    if *show_visualizer {
        visualizer.show_window(egui_contexts.ctx_mut());
    }
}

fn player_input(keyboard_input: Res<ButtonInput<KeyCode>>, mut player_input: ResMut<PlayerInput>) {
    player_input.left = keyboard_input.pressed(KeyCode::ArrowLeft);
    player_input.right = keyboard_input.pressed(KeyCode::ArrowRight);
    player_input.up = keyboard_input.pressed(KeyCode::ArrowUp);
    player_input.down = keyboard_input.pressed(KeyCode::ArrowDown);
}

fn client_send_input(player_input: Res<PlayerInput>, mut client: ResMut<RenetClient>) {
    let input_message = bincode::serialize(&*player_input).unwrap();
    client.send_message(ClientChannel::Input, input_message);
}

fn client_send_player_commands(
    mut player_commands: EventReader<PlayerCommand>,
    mut client: ResMut<RenetClient>,
) {
    for command in player_commands.read() {
        let command_message = bincode::serialize(command).unwrap();
        client.send_message(ClientChannel::Command, command_message);
    }
}

fn client_sync_players(
    mut cmd: Commands,
    mut client: ResMut<RenetClient>,
    transport: Res<NetcodeClientTransport>,
    mut lobby: ResMut<ClientLobby>,
    mut network_mapping: ResMut<NetworkMapping>,
    car_res: Res<bevy_garage_car::CarRes>,
    car_wheels: Query<&CarWheels>,
    mut wheel_query: Query<&mut Transform, With<Wheel>>,
) {
    let client_id = transport.client_id();
    while let Some(message) = client.receive_message(ServerChannel::ServerMessages) {
        let server_message = bincode::deserialize(&message).unwrap();
        match server_message {
            ServerMessages::PlayerCreate {
                id,
                translation,
                entity,
            } => {
                println!("Player {} connected.", id);

                let is_player = client_id == id;

                // let transform: Transform = Transform::from_translation(translation);
                let transform: Transform =
                    Transform::from_xyz(translation[0], translation[1], translation[2]);
                let client_entity = bevy_garage_car::spawn_car(
                    &mut cmd,
                    &car_res.car_scene.as_ref().unwrap(),
                    &car_res.wheel_scene.as_ref().unwrap(),
                    is_player,
                    transform,
                );

                if is_player {
                    cmd.entity(client_entity).insert(ControlledPlayer);
                }

                let player_info = PlayerInfo {
                    server_entity: entity,
                    client_entity,
                };
                lobby.players.insert(id, player_info);
                network_mapping.0.insert(entity, client_entity);
            }
            ServerMessages::PlayerRemove { id } => {
                println!("Player {} disconnected.", id);
                if let Some(PlayerInfo {
                    server_entity,
                    client_entity,
                }) = lobby.players.remove(&id)
                {
                    cmd.entity(client_entity).despawn();
                    network_mapping.0.remove(&server_entity);
                }
            }
        }
    }

    while let Some(message) = client.receive_message(ServerChannel::NetworkedEntities) {
        let networked_entities: NetworkedEntities = bincode::deserialize(&message).unwrap();

        for i in 0..networked_entities.entities.len() {
            if let Some(entity) = network_mapping.0.get(&networked_entities.entities[i]) {
                let translation = networked_entities.translations[i].into();
                let rotation: Quat = Quat::from_array(networked_entities.rotations[i]);
                let transform = Transform {
                    translation,
                    rotation,
                    ..Default::default()
                };
                cmd.entity(*entity).insert(transform);

                let translations = networked_entities.wheels_translations[i];
                let rotations = networked_entities.wheels_rotations[i];

                let car_wheels = car_wheels.get(*entity);
                if let Ok(car_wheels) = car_wheels {
                    for (i, e) in car_wheels.entities.iter().enumerate() {
                        let mut wheel_transform = wheel_query.get_mut(*e).unwrap();
                        wheel_transform.translation = translations[i].into();
                        wheel_transform.rotation = Quat::from_array(rotations[i]);
                    }
                }
            }
        }
    }
}
