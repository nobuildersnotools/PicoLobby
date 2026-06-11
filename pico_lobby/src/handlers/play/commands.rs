use crate::handlers::play::set_player_position_and_rotation::teleport_player_to_spawn;
use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::lobby_chat::{
    MAX_CHAT_MESSAGE_CHARS, chat_feedback_packet, escape_minimessage_text,
    plain_chat_feedback_packet,
};
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{LobbyPrivateMessageError, ServerCommand, ServerCommands, ServerState};
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
        consume_chat_attempt_if_enabled(client_state, server_state);
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

/// Minimum interval between two commands from the same client. Five commands a
/// second is far above any human cadence but caps automated command floods,
/// each of which can generate outbound packets (transfers, `BungeeCord` connects)
/// and log lines.
const COMMAND_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

fn run_command(
    client_state: &mut ClientState,
    server_state: &ServerState,
    command: &str,
    batch: &mut Batch<PacketRegistry>,
) {
    // Drop floods silently; sending feedback here would itself be an amplifier.
    if !client_state.check_command_rate_limit(COMMAND_MIN_INTERVAL) {
        return;
    }

    info!(
        "{} issued server command: /{}",
        client_state.get_username(),
        command
    );

    match Command::parse(server_state.server_commands(), command) {
        Ok(parsed_command) => match parsed_command {
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
                        port: VarInt::from(i32::from(port)),
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
            Command::Msg { target, message } => {
                handle_private_message_command(
                    client_state,
                    server_state,
                    &target,
                    &message,
                    batch,
                );
            }
            Command::Reply { message } => {
                handle_reply_command(client_state, server_state, &message, batch);
            }
        },
        Err(ParseCommandError::Unknown) => {}
        Err(err) => {
            let version = client_state.protocol_version();
            let msg = format!("Invalid command usage: {err}");
            batch.queue(move || plain_chat_feedback_packet(version, &msg));
        }
    }
}

fn handle_private_message_command(
    client_state: &mut ClientState,
    server_state: &ServerState,
    target: &str,
    message: &str,
    batch: &mut Batch<PacketRegistry>,
) {
    let settings = server_state.private_message_settings();
    let version = client_state.protocol_version();
    let message = message.trim();
    if message.is_empty() {
        consume_chat_attempt_if_enabled(client_state, server_state);
        let feedback = settings.empty_message.clone();
        batch.queue(move || chat_feedback_packet(version, &feedback));
        return;
    }
    if message.chars().count() > MAX_CHAT_MESSAGE_CHARS {
        consume_chat_attempt_if_enabled(client_state, server_state);
        let feedback = settings.too_long.clone();
        batch.queue(move || chat_feedback_packet(version, &feedback));
        return;
    }

    let antispam = server_state.chat_antispam();
    if antispam.enabled && !client_state.check_chat_rate_limit(antispam.chat_cooldown) {
        let feedback = settings.rate_limit.clone();
        batch.queue(move || chat_feedback_packet(version, &feedback));
        return;
    }

    let plan =
        match server_state.plan_lobby_private_message(client_state, target, message.to_owned()) {
            Ok(plan) => plan,
            Err(err) => {
                queue_private_message_error(batch, version, settings, err, target);
                return;
            }
        };

    client_state.set_pending_private_message_plan(plan);
}

fn handle_reply_command(
    client_state: &mut ClientState,
    server_state: &ServerState,
    message: &str,
    batch: &mut Batch<PacketRegistry>,
) {
    let settings = server_state.private_message_settings();
    let version = client_state.protocol_version();
    let message = message.trim();
    if message.is_empty() {
        consume_chat_attempt_if_enabled(client_state, server_state);
        let feedback = settings.empty_message.clone();
        batch.queue(move || chat_feedback_packet(version, &feedback));
        return;
    }
    if message.chars().count() > MAX_CHAT_MESSAGE_CHARS {
        consume_chat_attempt_if_enabled(client_state, server_state);
        let feedback = settings.too_long.clone();
        batch.queue(move || chat_feedback_packet(version, &feedback));
        return;
    }

    if let Err(err) = server_state.validate_lobby_reply_target(client_state) {
        queue_private_message_error(batch, version, settings, err, "");
        return;
    }

    let antispam = server_state.chat_antispam();
    if antispam.enabled && !client_state.check_chat_rate_limit(antispam.chat_cooldown) {
        let feedback = settings.rate_limit.clone();
        batch.queue(move || chat_feedback_packet(version, &feedback));
        return;
    }

    let plan = match server_state.plan_lobby_reply_message(client_state, message.to_owned()) {
        Ok(plan) => plan,
        Err(err) => {
            queue_private_message_error(batch, version, settings, err, "");
            return;
        }
    };

    client_state.set_pending_private_message_plan(plan);
}

fn queue_private_message_error(
    batch: &mut Batch<PacketRegistry>,
    version: ProtocolVersion,
    settings: &crate::server_state::PrivateMessageSettings,
    err: LobbyPrivateMessageError,
    target: &str,
) {
    let template = match err {
        LobbyPrivateMessageError::Unavailable => &settings.unavailable,
        LobbyPrivateMessageError::UnknownTarget => &settings.unknown_target,
        LobbyPrivateMessageError::AmbiguousTarget => &settings.ambiguous_target,
        LobbyPrivateMessageError::HiddenTarget => &settings.hidden_target,
        LobbyPrivateMessageError::MissingReplyTarget => &settings.missing_reply_target,
        LobbyPrivateMessageError::SelfMessage => &settings.self_message,
    };
    #[allow(clippy::literal_string_with_formatting_args)]
    let feedback = template.replace("{target}", &escape_minimessage_text(target));
    batch.queue(move || chat_feedback_packet(version, &feedback));
}

fn consume_chat_attempt_if_enabled(client_state: &mut ClientState, server_state: &ServerState) {
    if server_state.chat_antispam().enabled {
        client_state.consume_chat_rate_limit();
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
    InvalidPort,
    #[error("missing destination id")]
    MissingDestinationId,
    #[error("missing private-message target")]
    MissingPrivateMessageTarget,
}

#[derive(Debug, PartialEq)]
enum Command {
    Spawn,
    Fly,
    FlySpeed(f32),
    Transfer(String, u16),
    Server(String),
    Msg { target: String, message: String },
    Reply { message: String },
}

impl Command {
    pub fn parse(server_commands: &ServerCommands, input: &str) -> Result<Self, ParseCommandError> {
        let (cmd, rest) = split_first_word(input).ok_or(ParseCommandError::Empty)?;
        if Self::is_command(server_commands.spawn(), cmd) {
            Ok(Self::Spawn)
        } else if Self::is_command(server_commands.fly(), cmd) {
            Ok(Self::Fly)
        } else if Self::is_command(server_commands.fly_speed(), cmd) {
            let mut parts = rest.split_whitespace();
            let speed_str = parts.next().unwrap_or("0.05");
            let speed = speed_str.parse::<f32>()?.clamp(0.0, 1.0);
            Ok(Self::FlySpeed(speed))
        } else if Self::is_command(server_commands.transfer(), cmd) {
            let mut parts = rest.split_whitespace();
            let host = parts
                .next()
                .ok_or(ParseCommandError::InvalidHost)?
                .to_string();
            let port_str = parts.next().unwrap_or("25565");
            let port = port_str
                .parse::<u16>()
                .map_err(|_| ParseCommandError::InvalidPort)?;
            Ok(Self::Transfer(host, port))
        } else if Self::is_command(server_commands.server(), cmd) {
            let id = rest
                .split_whitespace()
                .next()
                .ok_or(ParseCommandError::MissingDestinationId)?
                .to_string();
            Ok(Self::Server(id))
        } else if Self::is_command(server_commands.msg(), cmd) {
            let (target, message) =
                split_first_word(rest).ok_or(ParseCommandError::MissingPrivateMessageTarget)?;
            Ok(Self::Msg {
                target: target.to_string(),
                message: message.trim().to_string(),
            })
        } else if Self::is_command(server_commands.reply(), cmd)
            || Self::is_any_command(server_commands.reply_aliases(), cmd)
        {
            Ok(Self::Reply {
                message: rest.trim().to_string(),
            })
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

    fn is_any_command(server_commands: Vec<ServerCommand>, command: &str) -> bool {
        server_commands
            .into_iter()
            .any(|server_command| Self::is_command(server_command, command))
    }
}

fn split_first_word(input: &str) -> Option<(&str, &str)> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    let word_end = input.find(char::is_whitespace).unwrap_or(input.len());
    let word = &input[..word_end];
    let rest = input[word_end..].trim();
    Some((word, rest))
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
        client_named("TestPlayer", Uuid::from_u128(1))
    }

    fn client_named(username: &str, uuid: Uuid) -> ClientState {
        let mut c = ClientState::default();
        c.set_protocol_version(ProtocolVersion::V1_20_5);
        c.set_game_profile(GameProfile::new(username, uuid, None));
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

    #[test]
    fn parses_msg_with_greedy_message() {
        let commands = ServerCommands::from(CommandsConfig::default());

        let parsed = Command::parse(&commands, "msg Steve hello there").unwrap();

        assert_eq!(
            parsed,
            Command::Msg {
                target: "Steve".to_string(),
                message: "hello there".to_string(),
            }
        );
    }

    #[test]
    fn parses_msg_with_extra_spaces_trimmed() {
        let commands = ServerCommands::from(CommandsConfig::default());

        let parsed = Command::parse(&commands, "  msg   Steve    hello there  ").unwrap();

        assert_eq!(
            parsed,
            Command::Msg {
                target: "Steve".to_string(),
                message: "hello there".to_string(),
            }
        );
    }

    #[test]
    fn parses_reply_aliases() {
        let commands = ServerCommands::from(CommandsConfig::default());

        assert_eq!(
            Command::parse(&commands, "reply hello").unwrap(),
            Command::Reply {
                message: "hello".to_string()
            }
        );
        assert_eq!(
            Command::parse(&commands, "r hello").unwrap(),
            Command::Reply {
                message: "hello".to_string()
            }
        );
    }

    #[test]
    fn transfer_port_must_fit_u16() {
        let commands = ServerCommands::from(CommandsConfig::default());

        assert!(matches!(
            Command::parse(&commands, "transfer example.org -1"),
            Err(ParseCommandError::InvalidPort)
        ));
        assert!(matches!(
            Command::parse(&commands, "transfer example.org 70000"),
            Err(ParseCommandError::InvalidPort)
        ));
        assert_eq!(
            Command::parse(&commands, "transfer example.org 25565").unwrap(),
            Command::Transfer("example.org".to_string(), 25565)
        );
    }

    #[tokio::test]
    async fn invalid_known_command_sends_feedback() {
        let mut client = client();
        let server = server();
        let mut batch = Batch::new();

        run_command(
            &mut client,
            &server,
            "transfer example.org 70000",
            &mut batch,
        );

        let packets = batch.into_stream().collect::<Vec<_>>().await;
        assert_eq!(packets.len(), 1);
    }

    #[test]
    fn disabled_msg_and_reply_do_not_parse() {
        let commands = ServerCommands::from(CommandsConfig {
            msg: String::new(),
            reply: String::new(),
            reply_aliases: Vec::new(),
            ..CommandsConfig::default()
        });

        assert!(matches!(
            Command::parse(&commands, "msg Steve hello"),
            Err(ParseCommandError::Unknown)
        ));
        assert!(matches!(
            Command::parse(&commands, "reply hello"),
            Err(ParseCommandError::Unknown)
        ));
        assert!(matches!(
            Command::parse(&commands, "r hello"),
            Err(ParseCommandError::Unknown)
        ));
    }

    #[test]
    fn private_message_command_creates_private_plan_only() {
        let mut sender = client_named("Sender", Uuid::from_u128(1));
        let mut recipient = client_named("Steve", Uuid::from_u128(2));
        let server = server_with_antispam(AntispamConfig {
            enabled: false,
            ..AntispamConfig::default()
        });
        server.register_lobby_session(&mut sender);
        server.register_lobby_session(&mut recipient);

        run_command(
            &mut sender,
            &server,
            "msg Steve hello there",
            &mut Batch::new(),
        );

        assert!(sender.take_pending_chat_plan().is_none());
        let plan = sender
            .take_pending_private_message_plan()
            .expect("private message plan");
        assert_eq!(plan.sender_username, "Sender");
        assert_eq!(plan.recipient_username, "Steve");
        assert_eq!(plan.message, "hello there");
    }

    #[test]
    fn overlong_private_message_is_rejected() {
        let mut sender = client_named("Sender", Uuid::from_u128(1));
        let mut recipient = client_named("Steve", Uuid::from_u128(2));
        let server = server();
        server.register_lobby_session(&mut sender);
        server.register_lobby_session(&mut recipient);

        run_command(
            &mut sender,
            &server,
            &format!("msg Steve {}", "a".repeat(MAX_CHAT_MESSAGE_CHARS + 1)),
            &mut Batch::new(),
        );

        assert!(sender.take_pending_private_message_plan().is_none());
    }

    #[test]
    fn private_messages_obey_chat_antispam() {
        let mut sender = client_named("Sender", Uuid::from_u128(1));
        let mut recipient = client_named("Steve", Uuid::from_u128(2));
        let server = server();
        server.register_lobby_session(&mut sender);
        server.register_lobby_session(&mut recipient);

        run_command(&mut sender, &server, "msg Steve first", &mut Batch::new());
        sender.take_pending_private_message_plan();
        run_command(&mut sender, &server, "msg Steve second", &mut Batch::new());

        assert!(sender.take_pending_private_message_plan().is_none());
    }
}
