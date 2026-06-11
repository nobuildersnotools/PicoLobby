use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;
use minecraft_packets::play::player_input_packet::PlayerInputPacket;

impl PacketHandler for PlayerInputPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        let Some(shift) = self.shift() else {
            return Ok(Batch::new());
        };

        if let Some(plan) =
            server_state.update_lobby_crouching_with_metadata_plan(client_state, shift)
        {
            client_state.set_pending_metadata_plan(plan);
        }

        Ok(Batch::new())
    }
}
