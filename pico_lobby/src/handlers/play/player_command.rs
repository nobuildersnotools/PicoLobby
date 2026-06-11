use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;
use minecraft_packets::play::player_command_packet::PlayerCommandPacket;

impl PacketHandler for PlayerCommandPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        let Some(crouching) = self.crouching_change() else {
            return Ok(Batch::new());
        };

        if self.entity_id() != client_state.entity_id() {
            return Ok(Batch::new());
        }

        if let Some(plan) =
            server_state.update_lobby_crouching_with_metadata_plan(client_state, crouching)
        {
            client_state.set_pending_metadata_plan(plan);
        }

        Ok(Batch::new())
    }
}
