use crate::handlers::play::set_player_position_and_rotation::teleport_player_to_spawn;
use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::lobby_chat::{
    MAX_CHAT_MESSAGE_CHARS, chat_feedback_packet, plain_chat_feedback_packet,
};
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{ServerCommand, ServerCommands, ServerState};
use minecraft_packets::play::chat_command_packet::ChatCommandPacket;
use minecraft_packets::play::chat_message_packet::ChatMessagePacket;
use minecraft_packets::play::client_bound_player_abilities_packet::ClientBoundPlayerAbilitiesPacket;
use minecraft_packets::play::client_bound_plugin_message_packet::PlayClientBoundPluginMessagePacket;
use minecraft_packets::play::transfer_packet::TransferPacket;
use minecraft_protocol::prelude::{ProtocolVersion, VarInt};
use thiserror::Error;
use tracing::{info, warn};

impl PacketHandler for ChatCommandPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        let mut batch = Batch::new();
        run_command(client_state, server_state, self.get_command(), &mut batch);
        Ok(batch)
    }
}

impl PacketHandler for ChatMessagePacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        let mut batch = Batch::new();
        if let Some(command) = self.get_command() {
            run_command(client_state, server_state, command, &mut batch);
        } else {
            handle_chat_message(client_state, server_state, self.get_message(), &mut batch);
        }
        Ok(batch)
    }
}

fn handle_chat_message(
    client_state: &mut ClientState,
    server_state: &ServerState,
    message: &str,
    batch: &mut Batch<PacketRegistry>,
) {
    if message.chars().count() > MAX_CHAT_MESSAGE_CHARS {
        let version = client_state.protocol_version();
        batch.queue(move || chat_feedback_packet(version, "Chat message is too long."));
        return;
    }

    let antispam = server_state.chat_antispam();
    if antispam.enabled && !client_state.check_chat_rate_limit(antispam.chat_cooldown) {
        let version = client_state.protocol_version();
        let message = antispam.message.clone();
        batch.queue(move || chat_feedback_packet(version, &message));
        return;
    }

    info!("<{}> {}", client_state.get_username(), message);
    if let Some(plan) = server_state.plan_lobby_chat_broadcast(client_state, message.to_owned()) {
        client_state.set_pending_chat_plan(plan);
    }
}

fn run_command(
    client_state: &mut ClientState,
    server_state: &ServerState,
    command: &str,
    batch: &mut Batch<PacketRegistry>,
) {
    info!(
        "{} issued server command: /{}",
        client_state.get_username(),
        command
    );

    if let Ok(parsed_command) = Command::parse(server_state.server_commands(), command) {
        match parsed_command {
            Command::Spawn => {
                teleport_player_to_spawn(client_state, server_state, batch);
            }
            Command::Fly => {
                let allow_flying = !client_state.is_flight_allowed();
                let flying = allow_flying && client_state.is_flying();
                let packet = ClientBoundPlayerAbilitiesPacket::builder()
                    .allow_flying(allow_flying)
                    .flying(flying)
                    .flying_speed(client_state.get_flying_speed())
                    .build();
                batch.queue(|| PacketRegistry::ClientBoundPlayerAbilities(packet));
                client_state.set_is_flight_allowed(allow_flying);
                client_state.set_is_flying(allow_flying);
            }
            Command::FlySpeed(speed) => {
                let packet = ClientBoundPlayerAbilitiesPacket::builder()
                    .allow_flying(client_state.is_flight_allowed())
                    .flying(client_state.is_flying())
                    .flying_speed(speed)
                    .build();
                batch.queue(|| PacketRegistry::ClientBoundPlayerAbilities(packet));
                client_state.set_flying_speed(speed);
            }
            Command::Transfer(host, port) => {
                if client_state
                    .protocol_version()
                    .is_after_inclusive(ProtocolVersion::V1_20_5)
                {
                    info!(
                        "Transferring {} to {}:{}",
                        client_state.get_username(),
                        host,
                        port
                    );
                    let packet = TransferPacket {
                        host,
                        port: VarInt::from(port),
                    };
                    batch.queue(|| PacketRegistry::Transfer(packet));
                } else {
                    warn!(
                        "{} tried to transfer servers on unsupported version {}",
                        client_state.get_username(),
                        client_state.protocol_version().humanize()
                    );
                }
            }
            Command::Server(destination_id) => {
                let version = client_state.protocol_version();
                match server_state.resolve_lobby_destination(&destination_id) {
                    Ok(dest) => {
                        info!(
                            "Sending {} to Velocity server '{}' via /server {}",
                            client_state.get_username(),
                            dest.server,
                            destination_id,
                        );
                        let packet =
                            PlayClientBoundPluginMessagePacket::bungeecord_connect(&dest.server);
                        batch.queue(|| PacketRegistry::PlayClientBoundPluginMessage(packet));
                    }
                    Err(err) => {
                        warn!("{}: {}", client_state.get_username(), err);
                        let msg = format!("Unknown server: {destination_id}");
                        batch.queue(move || plain_chat_feedback_packet(version, &msg));
                    }
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ParseCommandError {
    #[error("empty command")]
    Empty,
    #[error("unknown command")]
    Unknown,
    #[error("invalid speed value")]
    InvalidSpeed(#[from] std::num::ParseFloatError),
    #[error("invalid hostname")]
    InvalidHost,
    #[error("invalid port")]
    InvalidPort(#[from] std::num::ParseIntError),
    #[error("missing destination id")]
    MissingDestinationId,
}

enum Command {
    Spawn,
    Fly,
    FlySpeed(f32),
    Transfer(String, i32),
    Server(String),
}

impl Command {
    pub fn parse(server_commands: &ServerCommands, input: &str) -> Result<Self, ParseCommandError> {
        let mut parts = input.split_whitespace();
        let cmd = parts.next().ok_or(ParseCommandError::Empty)?;
        if Self::is_command(server_commands.spawn(), cmd) {
            Ok(Self::Spawn)
        } else if Self::is_command(server_commands.fly(), cmd) {
            Ok(Self::Fly)
        } else if Self::is_command(server_commands.fly_speed(), cmd) {
            let speed_str = parts.next().unwrap_or("0.05");
            let speed = speed_str.parse::<f32>()?.clamp(0.0, 1.0);
            Ok(Self::FlySpeed(speed))
        } else if Self::is_command(server_commands.transfer(), cmd) {
            let host = parts
                .next()
                .ok_or(ParseCommandError::InvalidHost)?
                .to_string();
            let port_str = parts.next().unwrap_or("25565");
            let port = port_str.parse::<i32>()?;
            Ok(Self::Transfer(host, port))
        } else if Self::is_command(server_commands.server(), cmd) {
            let id = parts
                .next()
                .ok_or(ParseCommandError::MissingDestinationId)?
                .to_string();
            Ok(Self::Server(id))
        } else {
            Err(ParseCommandError::Unknown)
        }
    }

    fn is_command(server_command: ServerCommand, command: &str) -> bool {
        if let ServerCommand::Enabled { alias } = server_command
            && command == alias
        {
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::antispam::AntispamConfig;
    use crate::configuration::commands::CommandsConfig;
    use crate::server::game_profile::GameProfile;
    use futures::StreamExt;
    use minecraft_protocol::prelude::Uuid;
    use std::time::Duration;

    fn client() -> ClientState {
        let mut c = ClientState::default();
        c.set_protocol_version(ProtocolVersion::V1_20_5);
        c.set_game_profile(GameProfile::new("TestPlayer", Uuid::from_u128(1), None));
        c
    }

    fn server() -> ServerState {
        server_with_antispam(AntispamConfig::default())
    }

    fn server_with_antispam(antispam: AntispamConfig) -> ServerState {
        let mut builder = ServerState::builder();
        builder
            .set_lobby_enabled(true)
            .antispam(antispam)
            .show_online_player_count(true)
            .server_commands(CommandsConfig::default());
        builder.build().unwrap()
    }

    #[test]
    fn overlong_message_does_not_broadcast() {
        let mut client = client();
        let server = server();
        let mut batch = Batch::new();
        let long_message = "a".repeat(MAX_CHAT_MESSAGE_CHARS + 1);

        handle_chat_message(&mut client, &server, &long_message, &mut batch);

        assert!(client.take_pending_chat_plan().is_none());
    }

    #[test]
    fn rate_limited_second_message_does_not_broadcast() {
        let mut client = client();
        let server = server();
        server.register_lobby_session(&mut client);

        handle_chat_message(&mut client, &server, "first", &mut Batch::new());
        client.take_pending_chat_plan();

        handle_chat_message(&mut client, &server, "second", &mut Batch::new());

        assert!(client.take_pending_chat_plan().is_none());
    }

    #[test]
    fn first_message_creates_lobby_chat_plan() {
        let mut client = client();
        let server = server();
        server.register_lobby_session(&mut client);

        handle_chat_message(&mut client, &server, "hello", &mut Batch::new());

        let plan = client.take_pending_chat_plan().expect("chat plan");
        assert_eq!(plan.message, "hello");
    }

    #[test]
    fn message_after_configured_cooldown_broadcasts() {
        let mut client = client();
        let server = server_with_antispam(AntispamConfig {
            chat_cooldown_ms: 1,
            ..AntispamConfig::default()
        });
        server.register_lobby_session(&mut client);

        handle_chat_message(&mut client, &server, "first", &mut Batch::new());
        client.take_pending_chat_plan();
        std::thread::sleep(Duration::from_millis(2));

        handle_chat_message(&mut client, &server, "second", &mut Batch::new());

        let plan = client.take_pending_chat_plan().expect("chat plan");
        assert_eq!(plan.message, "second");
    }

    #[test]
    fn disabled_antispam_allows_rapid_messages() {
        let mut client = client();
        let server = server_with_antispam(AntispamConfig {
            enabled: false,
            ..AntispamConfig::default()
        });
        server.register_lobby_session(&mut client);

        handle_chat_message(&mut client, &server, "first", &mut Batch::new());
        client.take_pending_chat_plan();
        handle_chat_message(&mut client, &server, "second", &mut Batch::new());

        let plan = client.take_pending_chat_plan().expect("chat plan");
        assert_eq!(plan.message, "second");
    }

    #[tokio::test]
    async fn rate_limited_message_sends_feedback_packet() {
        let mut client = client();
        let server = server_with_antispam(AntispamConfig {
            message: "Slow down.".to_string(),
            ..AntispamConfig::default()
        });
        server.register_lobby_session(&mut client);

        handle_chat_message(&mut client, &server, "first", &mut Batch::new());
        client.take_pending_chat_plan();

        let mut batch = Batch::new();
        handle_chat_message(&mut client, &server, "second", &mut batch);

        let packets = batch.into_stream().collect::<Vec<_>>().await;
        assert_eq!(packets.len(), 1);
    }

    #[test]
    fn command_does_not_broadcast_as_chat() {
        let mut client = client();
        let server = server();
        server.register_lobby_session(&mut client);

        run_command(&mut client, &server, "spawn", &mut Batch::new());

        assert!(client.take_pending_chat_plan().is_none());
    }

    #[test]
    fn chat_antispam_does_not_block_commands() {
        let mut client = client();
        let server = server();
        server.register_lobby_session(&mut client);

        handle_chat_message(&mut client, &server, "first", &mut Batch::new());
        client.take_pending_chat_plan();
        run_command(&mut client, &server, "fly", &mut Batch::new());

        assert!(client.is_flight_allowed());
    }
}
