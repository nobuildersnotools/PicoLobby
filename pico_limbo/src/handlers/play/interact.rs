use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::lobby_chat::chat_feedback_packet;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;
use minecraft_packets::play::attack_packet::AttackPacket;
use minecraft_packets::play::client_bound_plugin_message_packet::PlayClientBoundPluginMessagePacket;
use minecraft_packets::play::interact_packet::InteractPacket;
use tracing::{info, warn};

const NPC_INTERACTION_RANGE: f64 = 6.0;

impl PacketHandler for InteractPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        if !self.triggers_npc_interaction() {
            return Ok(Batch::new());
        }

        handle_npc_interaction(client_state, server_state, self.target_entity_id())
    }
}

impl PacketHandler for AttackPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        handle_npc_interaction(client_state, server_state, self.target_entity_id())
    }
}

fn handle_npc_interaction(
    client_state: &mut ClientState,
    server_state: &ServerState,
    target_entity_id: i32,
) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
    let Some(interaction) = server_state.plan_lobby_npc_interaction(
        client_state,
        target_entity_id,
        NPC_INTERACTION_RANGE,
    ) else {
        return Ok(Batch::new());
    };

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
            Ok(batch)
        }
        Err(err) => {
            warn!("{}: {}", client_state.get_username(), err);
            let msg = format!("Unknown server: {}", interaction.destination_id);
            let mut batch = Batch::new();
            batch.queue(move || chat_feedback_packet(version, &msg));
            Ok(batch)
        }
    }
}
