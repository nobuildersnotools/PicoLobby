use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::lobby_chat::plain_chat_feedback_packet;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;
use minecraft_packets::play::attack_packet::AttackPacket;
use minecraft_packets::play::client_bound_plugin_message_packet::PlayClientBoundPluginMessagePacket;
use minecraft_packets::play::interact_packet::InteractPacket;
use std::time::Duration;
use tracing::{info, warn};

const NPC_INTERACTION_RANGE: f64 = 6.0;

/// Minimum interval between two handled NPC interactions from the same client.
/// Each accepted interaction sends a server-connect plugin message to the proxy,
/// so without this an attack/interact spam (trivial for a cheat client) would
/// flood the proxy with connect requests and the log with lines. Well above any
/// human click cadence.
const NPC_INTERACTION_MIN_INTERVAL: Duration = Duration::from_millis(250);

impl PacketHandler for InteractPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        if !self.triggers_npc_interaction() {
            return Ok(Batch::new());
        }

        Ok(handle_npc_interaction(
            client_state,
            server_state,
            self.target_entity_id(),
        ))
    }
}

impl PacketHandler for AttackPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        Ok(handle_npc_interaction(
            client_state,
            server_state,
            self.target_entity_id(),
        ))
    }
}

fn handle_npc_interaction(
    client_state: &mut ClientState,
    server_state: &ServerState,
    target_entity_id: i32,
) -> Batch<PacketRegistry> {
    let Some(interaction) = server_state.plan_lobby_npc_interaction(
        client_state,
        target_entity_id,
        NPC_INTERACTION_RANGE,
    ) else {
        return Batch::new();
    };

    // Throttle only confirmed NPC interactions: clicking empty space or another
    // entity resolves to `None` above and never consumes the interaction budget.
    if !client_state.check_interaction_rate_limit(NPC_INTERACTION_MIN_INTERVAL) {
        return Batch::new();
    }

    let version = client_state.protocol_version();
    match server_state.resolve_lobby_destination(&interaction.destination_id) {
        Ok(dest) => {
            info!(
                "Sending {} to Velocity server '{}' via NPC '{}'",
                client_state.get_username(),
                dest.server,
                interaction.npc_id.as_str(),
            );
            let packet = PlayClientBoundPluginMessagePacket::bungeecord_connect(&dest.server);
            let mut batch = Batch::new();
            batch.queue(|| PacketRegistry::PlayClientBoundPluginMessage(packet));
            batch
        }
        Err(err) => {
            warn!("{}: {}", client_state.get_username(), err);
            let msg = format!("Unknown server: {}", interaction.destination_id);
            let mut batch = Batch::new();
            batch.queue(move || plain_chat_feedback_packet(version, &msg));
            batch
        }
    }
}
