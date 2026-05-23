use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;
use minecraft_packets::play::swing_packet::SwingPacket;
use std::time::Duration;

const SWING_BROADCAST_MIN_INTERVAL: Duration = Duration::from_millis(100);

impl PacketHandler for SwingPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        if !self.triggers_main_hand_swing(client_state.entity_id(), client_state.protocol_version())
        {
            return Ok(Batch::new());
        }

        if client_state.check_swing_broadcast_rate_limit(SWING_BROADCAST_MIN_INTERVAL)
            && let Some(plan) = server_state.plan_lobby_swing_broadcast(client_state)
        {
            client_state.set_pending_swing_plan(plan);
        }

        Ok(Batch::new())
    }
}
