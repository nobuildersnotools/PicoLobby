use crate::handlers::configuration::build_scoreboard_update_packets_from_rendered;
use crate::server::batch::OutboundPacket;
use crate::server::client_data::ClientData;
use crate::server::lobby_chat::{
    chat_component_for_plan, chat_packet_for_version, lifecycle_message_component_for_plan,
    private_message_packets_for_plan,
};
use crate::server::lobby_visibility::{
    delayed_npc_tab_list_remove_batches_for_join, join_visibility_batches_for_existing,
    join_visibility_packets_for_newcomer, leave_visibility_batches, metadata_visibility_packets,
    movement_visibility_packets, npc_spawn_packets_for_join, swing_visibility_packets,
};
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::{
    PacketRegistry, PacketRegistryDecodeError, PacketRegistryEncodeError,
};
use crate::server::shutdown_signal::shutdown_signal;
use crate::server_state::{
    LobbyChatPlan, LobbyMetadataPlan, LobbyMovementPlan, LobbyNpcSpawnPlan,
    LobbyPrivateMessagePlan, LobbyRecipient, LobbySessionId, LobbySwingPlan, RenderedScoreboard,
    ServerState,
};
use futures::StreamExt;
use minecraft_packets::login::login_disconnect_packet::LoginDisconnectPacket;
use minecraft_packets::play::client_bound_keep_alive_packet::ClientBoundKeepAlivePacket;
use minecraft_packets::play::disconnect_packet::DisconnectPacket;
use minecraft_protocol::prelude::{ProtocolVersion, State};
use net::packet_stream::PacketStreamError;
use net::raw_packet::RawPacket;
use std::collections::HashMap;
use std::future::pending;
use std::num::TryFromIntError;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::time::{Duration, Interval};
use tracing::{debug, error, info, trace, warn};

const LOBBY_BROADCAST_QUEUE_CAPACITY: usize = 256;

pub struct Server {
    state: Arc<RwLock<ServerState>>,
    listen_address: String,
}

impl Server {
    pub fn new(listen_address: &impl ToString, state: ServerState) -> Self {
        Self {
            state: Arc::new(RwLock::new(state)),
            listen_address: listen_address.to_string(),
        }
    }

    pub async fn run(self) {
        let listener = match TcpListener::bind(&self.listen_address).await {
            Ok(sock) => sock,
            Err(err) => {
                error!("Failed to bind to {}: {}", self.listen_address, err);
                std::process::exit(1);
            }
        };

        info!("Listening on: {}", self.listen_address);
        tokio::spawn(scoreboard_broadcast_task(Arc::clone(&self.state)));
        self.accept(&listener).await;
    }

    pub async fn accept(self, listener: &TcpListener) {
        loop {
            tokio::select! {
                 accept_result = listener.accept() => {
                    match accept_result {
                        Ok((socket, addr)) => {
                            debug!("Accepted connection from {}", addr);
                        let state_clone = Arc::clone(&self.state);
                            tokio::spawn(async move {
                                handle_client(socket, state_clone).await;
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept a connection: {:?}", e);
                        }
                    }
                },

                 () = shutdown_signal() => {
                    info!("Shutdown signal received, shutting down gracefully.");
                    break;
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum PacketProcessingError {
    #[error("Client disconnected")]
    Disconnected,

    #[error("Packet not found version={0} state={1} packet_id={2}")]
    DecodePacketError(i32, State, u8),

    #[error("{0}")]
    Custom(String),
}

impl From<PacketHandlerError> for PacketProcessingError {
    fn from(e: PacketHandlerError) -> Self {
        match e {
            PacketHandlerError::Custom(reason) => Self::Custom(reason),
            PacketHandlerError::InvalidState(reason, should_warn) => {
                if should_warn {
                    warn!("{reason}");
                } else {
                    debug!("{reason}");
                }
                Self::Disconnected
            }
        }
    }
}

impl From<PacketRegistryDecodeError> for PacketProcessingError {
    fn from(e: PacketRegistryDecodeError) -> Self {
        match e {
            PacketRegistryDecodeError::NoCorrespondingPacket(version, state, packet_id) => {
                Self::DecodePacketError(version, state, packet_id)
            }
            _ => Self::Custom(e.to_string()),
        }
    }
}

impl From<PacketRegistryEncodeError> for PacketProcessingError {
    fn from(e: PacketRegistryEncodeError) -> Self {
        Self::Custom(e.to_string())
    }
}

impl From<TryFromIntError> for PacketProcessingError {
    fn from(e: TryFromIntError) -> Self {
        Self::Custom(e.to_string())
    }
}

impl From<PacketStreamError> for PacketProcessingError {
    fn from(value: PacketStreamError) -> Self {
        match value {
            PacketStreamError::Io(ref e)
                if e.kind() == std::io::ErrorKind::UnexpectedEof
                    || e.kind() == std::io::ErrorKind::ConnectionReset =>
            {
                Self::Disconnected
            }
            _ => Self::Custom(value.to_string()),
        }
    }
}

async fn process_packet(
    client_data: &ClientData,
    server_state: &Arc<RwLock<ServerState>>,
    raw_packet: RawPacket,
    was_in_play_state: &mut bool,
    broadcast_tx: &mpsc::Sender<RawPacket>,
) -> Result<(), PacketProcessingError> {
    let mut client_state = client_data.client().await;
    let protocol_version = client_state.protocol_version();
    let state = client_state.state();
    let decoded_packet = PacketRegistry::decode_packet(protocol_version, state, raw_packet)?;

    let batch = {
        let server_state_guard = server_state.read().await;
        decoded_packet.handle(&mut client_state, &server_state_guard)?
    };

    let protocol_version = client_state.protocol_version();
    let state = client_state.state();

    let first_play_packet = !*was_in_play_state && state == State::Play;
    let mut join_session_id: Option<LobbySessionId> = None;

    if first_play_packet {
        *was_in_play_state = true;
        {
            let server_state_guard = server_state.read().await;
            if server_state_guard.lobby_enabled() {
                if let Some(session_id) = client_state.lobby_session_id() {
                    server_state_guard.set_lobby_broadcast_sender(session_id, broadcast_tx.clone());
                    join_session_id = Some(session_id);
                }
            } else {
                server_state_guard.increment();
            }
        }
        let username = client_state.get_username();
        let entity_id = client_state.entity_id();
        debug!(
            "{} joined using version {} with entity id {}",
            username,
            protocol_version.humanize(),
            entity_id
        );
        info!("{} joined the game with entity id {}", username, entity_id);
    }

    let mut stream = batch.into_outbound_stream();
    while let Some(pending_packet) = stream.next().await {
        let pending_packet = match pending_packet {
            OutboundPacket::Registry(packet) => packet,
            OutboundPacket::Raw(raw_packet) => {
                client_data.write_packet(raw_packet).await?;
                continue;
            }
        };
        let enable_compression = matches!(pending_packet, PacketRegistry::SetCompression(..));
        let raw_packet = pending_packet.encode_packet(protocol_version)?;
        client_data.write_packet(raw_packet).await?;
        if enable_compression
            && let Some(compression_settings) = server_state.read().await.compression_settings()
        {
            let mut packet_stream = client_data.stream().await;
            packet_stream
                .set_compression(compression_settings.threshold, compression_settings.level);
        }
    }

    let pending_metadata_plan = client_state.take_pending_metadata_plan();
    let pending_movement_plan = client_state.take_pending_movement_plan();
    let pending_swing_plan = client_state.take_pending_swing_plan();
    let pending_chat_plan = client_state.take_pending_chat_plan();
    let pending_private_message_plan = client_state.take_pending_private_message_plan();

    if let Some(reason) = client_state.should_kick() {
        drop(client_state);
        kick_client(client_data, reason.clone())
            .await
            .map_err(|_| PacketProcessingError::Disconnected)?;
        return Err(PacketProcessingError::Disconnected);
    }

    drop(client_state);

    if let Some(plan) = pending_metadata_plan {
        broadcast_metadata(&plan, server_state).await;
    }

    if let Some(plan) = pending_movement_plan {
        broadcast_movement(&plan, server_state).await;
    }

    if let Some(plan) = pending_swing_plan {
        broadcast_swing(&plan, server_state).await;
    }

    if let Some(plan) = pending_chat_plan {
        broadcast_chat(&plan, server_state).await;
    }

    if let Some(plan) = pending_private_message_plan {
        broadcast_private_message(&plan, server_state).await;
    }

    if let Some(session_id) = join_session_id {
        send_join_visibility(
            client_data,
            server_state,
            session_id,
            protocol_version,
            broadcast_tx,
        )
        .await?;
        broadcast_join_message(server_state, session_id).await;
    }

    client_data.enable_keep_alive_if_needed().await;

    Ok(())
}

async fn broadcast_join_message(
    server_state: &Arc<RwLock<ServerState>>,
    session_id: LobbySessionId,
) {
    let server_state_guard = server_state.read().await;
    let Some(plan) = server_state_guard.plan_lobby_join_message(session_id) else {
        return;
    };
    broadcast_lifecycle_message_with_guard(&server_state_guard, &plan);
    drop(server_state_guard);
}

async fn send_join_visibility(
    client_data: &ClientData,
    server_state: &Arc<RwLock<ServerState>>,
    session_id: LobbySessionId,
    newcomer_version: ProtocolVersion,
    broadcast_tx: &mpsc::Sender<RawPacket>,
) -> Result<(), PacketProcessingError> {
    let server_state_guard = server_state.read().await;
    let Some(join_plan) = server_state_guard.plan_lobby_join(session_id) else {
        return Ok(());
    };
    let npc_spawn_plan = server_state_guard.plan_lobby_npc_spawn();
    let senders =
        server_state_guard.collect_lobby_broadcast_senders(&join_plan.existing_recipients);
    drop(server_state_guard);

    let newcomer_packets = join_visibility_packets_for_newcomer(&join_plan, newcomer_version);
    let newcomer_packet_count = newcomer_packets.len();
    for packet in newcomer_packets {
        match packet.encode_packet(newcomer_version) {
            Ok(raw_packet) => client_data.write_packet(raw_packet).await?,
            Err(err) => {
                warn!(
                    "Failed to encode join visibility packet for newcomer version {}: {}",
                    newcomer_version.humanize(),
                    err
                );
            }
        }
    }

    if let Some(npc_spawn_plan) = npc_spawn_plan {
        for packet in npc_spawn_packets_for_join(&npc_spawn_plan, newcomer_version) {
            match packet.encode_packet(newcomer_version) {
                Ok(raw_packet) => client_data.write_packet(raw_packet).await?,
                Err(err) => {
                    warn!(
                        "Failed to encode NPC spawn packet for newcomer version {}: {}",
                        newcomer_version.humanize(),
                        err
                    );
                }
            }
        }

        schedule_delayed_npc_tab_list_removal(broadcast_tx, &npc_spawn_plan, newcomer_version);
    }

    let broadcast_batches = join_visibility_batches_for_existing(&join_plan);
    let mut broadcast_count = 0usize;
    for batch in broadcast_batches {
        let version = batch.recipient.protocol_version;
        let sid = batch.recipient.session_id;
        if let Some(sender) = senders.get(&sid) {
            for packet in batch.packets {
                match packet.encode_packet(version) {
                    Ok(raw_packet) => {
                        queue_broadcast_packet(sender, raw_packet, "join visibility");
                        broadcast_count += 1;
                    }
                    Err(err) => {
                        warn!(
                            "Failed to encode join visibility broadcast packet for session {:?} version {}: {}",
                            sid,
                            version.humanize(),
                            err
                        );
                    }
                }
            }
        }
    }

    debug!(
        "Join visibility: sent {} packets to newcomer ({} existing players), broadcast {} packets to {} existing clients for entity id {}",
        newcomer_packet_count,
        join_plan.existing_sessions.len(),
        broadcast_count,
        join_plan.existing_recipients.len(),
        join_plan.new_session.entity_id.get(),
    );

    Ok(())
}

fn schedule_delayed_npc_tab_list_removal(
    broadcast_tx: &mpsc::Sender<RawPacket>,
    npc_spawn_plan: &LobbyNpcSpawnPlan,
    newcomer_version: ProtocolVersion,
) {
    for batch in delayed_npc_tab_list_remove_batches_for_join(npc_spawn_plan, newcomer_version) {
        let raw_packets = batch
            .packets
            .into_iter()
            .filter_map(|packet| match packet.encode_packet(newcomer_version) {
                Ok(raw_packet) => Some(raw_packet),
                Err(err) => {
                    warn!(
                        "Failed to encode delayed NPC tab-list removal packet for newcomer version {}: {}",
                        newcomer_version.humanize(),
                        err
                    );
                    None
                }
            })
            .collect::<Vec<_>>();
        if raw_packets.is_empty() {
            continue;
        }

        let broadcast_tx = broadcast_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(batch.delay).await;
            for raw_packet in raw_packets {
                queue_broadcast_packet(&broadcast_tx, raw_packet, "NPC tab-list removal");
            }
        });
    }
}

async fn broadcast_movement(plan: &LobbyMovementPlan, server_state: &Arc<RwLock<ServerState>>) {
    if plan.recipients.is_empty() {
        return;
    }

    let server_state_guard = server_state.read().await;
    let senders = server_state_guard.collect_lobby_broadcast_senders(&plan.recipients);
    drop(server_state_guard);

    queue_version_bucketed_packets(&plan.recipients, &senders, "movement", |version| {
        movement_visibility_packets(
            version,
            plan.moving_entity_id,
            plan.previous_position,
            plan.current_position,
        )
    });
}

async fn broadcast_metadata(plan: &LobbyMetadataPlan, server_state: &Arc<RwLock<ServerState>>) {
    if plan.recipients.is_empty() {
        return;
    }

    let server_state_guard = server_state.read().await;
    let senders = server_state_guard.collect_lobby_broadcast_senders(&plan.recipients);
    drop(server_state_guard);

    queue_version_bucketed_packets(&plan.recipients, &senders, "metadata", |_| {
        metadata_visibility_packets(plan)
    });
}

async fn broadcast_swing(plan: &LobbySwingPlan, server_state: &Arc<RwLock<ServerState>>) {
    if plan.recipients.is_empty() {
        return;
    }

    let server_state_guard = server_state.read().await;
    let senders = server_state_guard.collect_lobby_broadcast_senders(&plan.recipients);
    drop(server_state_guard);

    queue_version_bucketed_packets(&plan.recipients, &senders, "swing", |_| {
        swing_visibility_packets(plan.swinging_entity_id)
    });
}

async fn broadcast_chat(plan: &LobbyChatPlan, server_state: &Arc<RwLock<ServerState>>) {
    if plan.recipients.is_empty() {
        return;
    }

    let server_state_guard = server_state.read().await;
    let senders = server_state_guard.collect_lobby_broadcast_senders(&plan.recipients);
    drop(server_state_guard);

    let component = chat_component_for_plan(plan);
    queue_version_bucketed_packets(&plan.recipients, &senders, "chat", |version| {
        vec![chat_packet_for_version(version, &component)]
    });
}

async fn broadcast_private_message(
    plan: &LobbyPrivateMessagePlan,
    server_state: &Arc<RwLock<ServerState>>,
) {
    let packets = private_message_packets_for_plan(plan);
    if packets.is_empty() {
        return;
    }

    let recipients = packets
        .iter()
        .map(|(recipient, _)| recipient.clone())
        .collect::<Vec<_>>();
    let server_state_guard = server_state.read().await;
    let senders = server_state_guard.collect_lobby_broadcast_senders(&recipients);
    drop(server_state_guard);

    for (recipient, packet) in packets {
        if let Some(sender) = senders.get(&recipient.session_id) {
            match packet.encode_packet(recipient.protocol_version) {
                Ok(raw_packet) => {
                    queue_broadcast_packet(sender, raw_packet, "private message");
                }
                Err(err) => {
                    warn!(
                        "Failed to encode private-message packet for session {:?} version {}: {}",
                        recipient.session_id,
                        recipient.protocol_version.humanize(),
                        err
                    );
                }
            }
        }
    }
}

fn broadcast_lifecycle_message_with_guard(
    server_state: &ServerState,
    plan: &crate::server_state::LobbyLifecycleMessagePlan,
) {
    if plan.recipients.is_empty() {
        return;
    }

    let senders = server_state.collect_lobby_broadcast_senders(&plan.recipients);
    let component = lifecycle_message_component_for_plan(plan);

    queue_version_bucketed_packets(&plan.recipients, &senders, "lifecycle message", |version| {
        vec![chat_packet_for_version(version, &component)]
    });
}

fn queue_version_bucketed_packets<F>(
    recipients: &[LobbyRecipient],
    senders: &HashMap<LobbySessionId, mpsc::Sender<RawPacket>>,
    context: &str,
    build_packets: F,
) where
    F: Fn(ProtocolVersion) -> Vec<PacketRegistry>,
{
    let mut buckets: HashMap<ProtocolVersion, Vec<&mpsc::Sender<RawPacket>>> = HashMap::new();
    for recipient in recipients {
        if let Some(sender) = senders.get(&recipient.session_id) {
            buckets
                .entry(recipient.protocol_version)
                .or_default()
                .push(sender);
        }
    }

    for (version, bucket_senders) in buckets {
        for packet in build_packets(version) {
            match packet.encode_packet(version) {
                Ok(raw_packet) => {
                    for sender in &bucket_senders {
                        queue_broadcast_packet(sender, raw_packet.clone(), context);
                    }
                }
                Err(err) => {
                    warn!(
                        "Failed to encode {context} broadcast packet for version {}: {}",
                        version.humanize(),
                        err
                    );
                }
            }
        }
    }
}

fn queue_broadcast_packet(sender: &mpsc::Sender<RawPacket>, raw_packet: RawPacket, context: &str) {
    match sender.try_send(raw_packet) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            trace!("Dropped lobby {context} packet for a client with a full broadcast queue");
        }
        Err(TrySendError::Closed(_)) => {
            trace!("Dropped lobby {context} packet for a closed broadcast queue");
        }
    }
}

async fn read(
    client_data: &ClientData,
    server_state: &Arc<RwLock<ServerState>>,
    was_in_play_state: &mut bool,
    broadcast_tx: &mpsc::Sender<RawPacket>,
    broadcast_rx: &mut mpsc::Receiver<RawPacket>,
    scoreboard_interval: &mut Option<Interval>,
    last_scoreboard_render: &mut Option<RenderedScoreboard>,
) -> Result<(), PacketProcessingError> {
    tokio::select! {
        result = client_data.read_packet() => {
            let raw_packet = result?;
            process_packet(client_data, server_state, raw_packet, was_in_play_state, broadcast_tx).await?;
        }
        () = client_data.keep_alive_tick() => {
            send_keep_alive(client_data).await?;
        }
        () = scoreboard_interval_tick(scoreboard_interval) => {
            refresh_scoreboard(client_data, server_state, last_scoreboard_render).await?;
        }
        Some(raw_packet) = broadcast_rx.recv() => {
            client_data.write_packet(raw_packet).await?;
        }
    }
    Ok(())
}

async fn configured_scoreboard_interval(
    server_state: &Arc<RwLock<ServerState>>,
) -> Option<Interval> {
    let (interval, lobby_enabled) = {
        let server_state_guard = server_state.read().await;
        (
            server_state_guard.scoreboard()?.update_interval(),
            server_state_guard.lobby_enabled(),
        )
    };
    if lobby_enabled {
        return None;
    }
    let mut interval = tokio::time::interval(interval.max(Duration::from_millis(50)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    Some(interval)
}

async fn scoreboard_broadcast_task(server_state: Arc<RwLock<ServerState>>) {
    let (tick_duration, enabled) = {
        let guard = server_state.read().await;
        let Some(scoreboard) = guard.scoreboard() else {
            return;
        };
        (scoreboard.update_interval(), guard.lobby_enabled())
    };
    if !enabled {
        return;
    }
    let mut interval = tokio::time::interval(tick_duration.max(Duration::from_millis(50)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut last_renders: std::collections::HashMap<LobbySessionId, RenderedScoreboard> =
        std::collections::HashMap::new();

    loop {
        interval.tick().await;
        let guard = server_state.read().await;
        let Some(scoreboard) = guard.scoreboard() else {
            continue;
        };
        let sessions = guard.lobby_scoreboard_sessions();
        if sessions.is_empty() {
            last_renders.clear();
            continue;
        }
        let online = guard.online_players();
        let max_players = guard.max_players();

        let recipients: Vec<_> = sessions
            .iter()
            .map(crate::server_state::ScoreboardSessionSnapshot::to_recipient)
            .collect();
        let senders = guard.collect_lobby_broadcast_senders(&recipients);

        let mut next_renders = std::collections::HashMap::with_capacity(sessions.len());

        for snap in &sessions {
            let placeholders = crate::server_state::ScoreboardPlaceholders {
                player: &snap.username,
                online,
                max_players,
                server: "lobby",
            };
            let rendered = match scoreboard.render(&placeholders) {
                Ok(r) => r,
                Err(err) => {
                    warn!(
                        "Failed to render scoreboard for session {:?}: {}",
                        snap.session_id, err
                    );
                    continue;
                }
            };
            let changed = last_renders.get(&snap.session_id) != Some(&rendered);
            if changed && let Some(sender) = senders.get(&snap.session_id) {
                let packets = build_scoreboard_update_packets_from_rendered(
                    rendered.clone(),
                    snap.protocol_version,
                );
                for packet in packets {
                    match packet.encode_packet(snap.protocol_version) {
                        Ok(raw) => {
                            queue_broadcast_packet(sender, raw, "scoreboard");
                        }
                        Err(err) => {
                            warn!(
                                "Failed to encode scoreboard packet for session {:?} version {}: {}",
                                snap.session_id,
                                snap.protocol_version.humanize(),
                                err
                            );
                        }
                    }
                }
            }
            next_renders.insert(snap.session_id, rendered);
        }
        drop(guard);
        last_renders = next_renders;
    }
}

async fn scoreboard_interval_tick(interval: &mut Option<Interval>) {
    if let Some(interval) = interval {
        interval.tick().await;
    } else {
        pending::<()>().await;
    }
}

async fn refresh_scoreboard(
    client_data: &ClientData,
    server_state: &Arc<RwLock<ServerState>>,
    last_scoreboard_render: &mut Option<RenderedScoreboard>,
) -> Result<(), PacketProcessingError> {
    let client_state = client_data.client().await;
    if client_state.state() != State::Play {
        return Ok(());
    }
    let protocol_version = client_state.protocol_version();
    let server_state_guard = server_state.read().await;
    let Some(scoreboard) = server_state_guard.scoreboard() else {
        return Ok(());
    };
    let username = client_state.get_username();
    let placeholders = crate::server_state::ScoreboardPlaceholders {
        player: &username,
        online: server_state_guard.online_players(),
        max_players: server_state_guard.max_players(),
        server: "lobby",
    };
    let rendered = scoreboard
        .render(&placeholders)
        .map_err(|err| PacketProcessingError::Custom(err.to_string()))?;
    drop(server_state_guard);
    if last_scoreboard_render.is_none() {
        *last_scoreboard_render = Some(rendered);
        return Ok(());
    }
    if last_scoreboard_render.as_ref() == Some(&rendered) {
        return Ok(());
    }
    let packets = build_scoreboard_update_packets_from_rendered(rendered.clone(), protocol_version);
    *last_scoreboard_render = Some(rendered);
    drop(client_state);

    for packet in packets {
        let raw_packet = packet.encode_packet(protocol_version)?;
        client_data.write_packet(raw_packet).await?;
    }
    Ok(())
}

async fn handle_client(socket: TcpStream, server_state: Arc<RwLock<ServerState>>) {
    let (broadcast_tx, mut broadcast_rx) =
        mpsc::channel::<RawPacket>(LOBBY_BROADCAST_QUEUE_CAPACITY);
    let client_data = ClientData::new(socket);
    let mut was_in_play_state = false;
    let mut scoreboard_interval = configured_scoreboard_interval(&server_state).await;
    let mut last_scoreboard_render = None;

    loop {
        match read(
            &client_data,
            &server_state,
            &mut was_in_play_state,
            &broadcast_tx,
            &mut broadcast_rx,
            &mut scoreboard_interval,
            &mut last_scoreboard_render,
        )
        .await
        {
            Ok(()) => {}
            Err(PacketProcessingError::Disconnected) => {
                debug!("Client disconnected");
                break;
            }
            Err(PacketProcessingError::Custom(e)) => {
                debug!("Error processing packet: {}", e);
            }
            Err(PacketProcessingError::DecodePacketError(version, state, packet_id)) => {
                trace!(
                    "Unknown packet received: version={version} state={state} packet_id={packet_id}"
                );
            }
        }
    }

    let _ = client_data.shutdown().await;

    if was_in_play_state {
        let client_state = client_data.client().await;
        let username = client_state.get_username();
        let lobby_session_id = client_state.lobby_session_id();
        drop(client_state);

        {
            let server_state_guard = server_state.read().await;
            if server_state_guard.lobby_enabled() {
                if let Some(plan) =
                    server_state_guard.unregister_lobby_session_with_leave_plan(lobby_session_id)
                {
                    let batches = leave_visibility_batches(&plan);
                    let senders =
                        server_state_guard.collect_lobby_broadcast_senders(&plan.recipients);

                    let batch_count = batches.len();
                    let mut sent = 0usize;
                    for batch in batches {
                        let session_id = batch.recipient.session_id;
                        let version = batch.recipient.protocol_version;
                        if let Some(sender) = senders.get(&session_id) {
                            for packet in batch.packets {
                                match packet.encode_packet(version) {
                                    Ok(raw_packet) => {
                                        queue_broadcast_packet(
                                            sender,
                                            raw_packet,
                                            "leave visibility",
                                        );
                                        sent += 1;
                                    }
                                    Err(err) => {
                                        warn!(
                                            "Failed to encode leave visibility packet for session {:?} version {}: {}",
                                            session_id,
                                            version.humanize(),
                                            err
                                        );
                                    }
                                }
                            }
                        }
                    }
                    debug!(
                        "Sent {} lobby leave visibility packets across {} recipients for entity id {}",
                        sent,
                        batch_count,
                        plan.departed_entity_id.get()
                    );
                    if let Some(message_plan) = server_state_guard.plan_lobby_leave_message(&plan) {
                        broadcast_lifecycle_message_with_guard(&server_state_guard, &message_plan);
                    }
                }
            } else {
                server_state_guard.decrement();
            }
        }
        info!("{} left the game", username);
    }
}

async fn kick_client(
    client_data: &ClientData,
    reason: String,
) -> Result<(), PacketProcessingError> {
    let (protocol_version, state) = {
        let state = client_data.client().await;
        (state.protocol_version(), state.state())
    };
    let packet = match state {
        State::Login => {
            debug!("Login disconnect");
            PacketRegistry::LoginDisconnect(LoginDisconnectPacket::text(reason))
        }
        State::Configuration => {
            debug!("Configuration disconnect");
            PacketRegistry::ConfigurationDisconnect(DisconnectPacket::text(reason))
        }
        State::Play => {
            debug!("Play disconnect");
            PacketRegistry::PlayDisconnect(DisconnectPacket::text(reason))
        }
        _ => {
            debug!("A user was disconnected from a state where no packet can be sent");
            return Err(PacketProcessingError::Disconnected);
        }
    };
    if let Ok(raw_packet) = packet.encode_packet(protocol_version) {
        client_data.write_packet(raw_packet).await?;
        client_data.shutdown().await?;
    }

    Ok(())
}

async fn send_keep_alive(client_data: &ClientData) -> Result<(), PacketProcessingError> {
    let (protocol_version, state) = {
        let client = client_data.client().await;
        (client.protocol_version(), client.state())
    };

    if state == State::Play {
        let packet = PacketRegistry::ClientBoundKeepAlive(ClientBoundKeepAlivePacket::random()?);
        let raw_packet = packet.encode_packet(protocol_version)?;
        client_data.write_packet(raw_packet).await?;
    }

    Ok(())
}
