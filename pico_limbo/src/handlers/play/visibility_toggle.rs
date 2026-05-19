use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::lobby_chat::chat_feedback_packet;
use crate::server::lobby_visibility::{
    join_visibility_packets_for_newcomer, leave_visibility_packets,
};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{LobbyJoinPlan, ServerState};

/// Called from the `UseItemPacket` and `LegacyUseItemPacket` handlers when the
/// player right-clicks the visibility toggle slot.
///
/// Returns `Some(batch)` if the toggle was handled (slot matched and toggle is
/// configured), or `None` if the caller should fall through.
pub fn handle_visibility_toggle(
    client_state: &mut ClientState,
    server_state: &ServerState,
) -> Option<Batch<PacketRegistry>> {
    let toggle = server_state.lobby_visibility_toggle()?;

    if client_state.selected_hotbar_slot() != toggle.hotbar_slot {
        return None;
    }

    let new_visible = client_state.toggle_players_visible();
    server_state.update_lobby_players_visible(client_state);
    let version = client_state.protocol_version();

    let join_plan: Option<LobbyJoinPlan> =
        server_state.collect_sessions_for_visibility_toggle(client_state);

    let mut batch = Batch::new();

    if let Some(plan) = join_plan {
        if new_visible {
            // Spawn all currently-online players for this client.
            let packets = join_visibility_packets_for_newcomer(&plan, version);
            for packet in packets {
                batch.queue(|| packet);
            }
        } else {
            // Despawn every other player from this client's view.
            for session in &plan.existing_sessions {
                let remove_packets = leave_visibility_packets(
                    version,
                    session.uuid,
                    &session.username,
                    session.entity_id,
                );
                for packet in remove_packets {
                    batch.queue(|| packet);
                }
            }
        }
    }

    // Refresh the hotbar item to reflect the new state.
    if let Some(slot_packet) = toggle.build_hotbar_packet(new_visible, version) {
        batch.queue(|| PacketRegistry::SetContainerSlot(slot_packet));
    }

    // Send the configurable feedback message.
    if let Some(message) = toggle.feedback_message(new_visible) {
        let message = message.to_owned();
        let feedback = chat_feedback_packet(version, &message);
        batch.queue(|| feedback);
    }

    Some(batch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::lobby::VisibilityToggleConfig;
    use crate::server::game_profile::GameProfile;
    use crate::server_state::{LobbyDestination, ServerState};
    use minecraft_protocol::prelude::{ProtocolVersion, Uuid};

    fn toggle_config(slot: u8) -> VisibilityToggleConfig {
        VisibilityToggleConfig {
            slot,
            item: "minecraft:ender_eye".to_string(),
            display_name_on: Some("<green>Visible".to_string()),
            display_name_off: Some("<red>Hidden".to_string()),
            lore_on: vec![],
            lore_off: vec![],
            message_on: Some("<green>Now visible.".to_string()),
            message_off: Some("<red>Now hidden.".to_string()),
        }
    }

    fn server_with_toggle(slot: u8) -> ServerState {
        let mut builder = ServerState::builder();
        builder.set_lobby_enabled(true);
        builder
            .set_lobby_visibility_toggle(Some(toggle_config(slot)))
            .unwrap();
        builder.build().unwrap()
    }

    fn server_without_toggle() -> ServerState {
        let mut builder = ServerState::builder();
        builder.set_lobby_enabled(true);
        builder.build().unwrap()
    }

    fn client(slot: u8) -> ClientState {
        let mut cs = ClientState::default();
        cs.set_protocol_version(ProtocolVersion::V1_21);
        cs.set_game_profile(GameProfile::new("Steve", Uuid::from_u128(1), None));
        cs.set_selected_hotbar_slot(slot);
        cs
    }

    #[test]
    fn no_toggle_configured_returns_none() {
        let server = server_without_toggle();
        let mut cs = client(8);
        assert!(handle_visibility_toggle(&mut cs, &server).is_none());
    }

    #[test]
    fn wrong_slot_returns_none() {
        let server = server_with_toggle(8);
        let mut cs = client(4); // toggle is on slot 8, client holds slot 4
        assert!(handle_visibility_toggle(&mut cs, &server).is_none());
    }

    #[test]
    fn correct_slot_flips_visibility_and_returns_batch() {
        let server = server_with_toggle(8);
        let mut cs = client(8);
        assert!(cs.players_visible());

        let batch = handle_visibility_toggle(&mut cs, &server);
        assert!(batch.is_some());
        assert!(!cs.players_visible(), "flag must be flipped to false");

        let batch2 = handle_visibility_toggle(&mut cs, &server);
        assert!(batch2.is_some());
        assert!(cs.players_visible(), "flag must flip back to true");
    }

    #[test]
    fn batch_contains_slot_packet_and_feedback() {
        let server = server_with_toggle(8);
        let mut cs = client(8);

        let batch = handle_visibility_toggle(&mut cs, &server).unwrap();
        let packets = batch.into_vec();

        let has_slot = packets
            .iter()
            .any(|p| matches!(p, PacketRegistry::SetContainerSlot(_)));
        let has_chat = packets.iter().any(|p| {
            matches!(
                p,
                PacketRegistry::SystemChatMessage(_) | PacketRegistry::LegacyChatMessage(_)
            )
        });

        assert!(has_slot, "expected SetContainerSlot in batch");
        assert!(has_chat, "expected chat feedback in batch");
    }

    #[test]
    fn toggle_with_server_destinations_includes_no_player_packets_when_lobby_empty() {
        let mut builder = ServerState::builder();
        builder.set_lobby_enabled(true);
        builder
            .set_lobby_destinations(vec![LobbyDestination::new("s", "S", "s")])
            .unwrap()
            .set_lobby_visibility_toggle(Some(toggle_config(8)))
            .unwrap();
        let server = builder.build().unwrap();

        // Register our own session in the lobby so collect_sessions works.
        let mut cs = client(8);
        server.register_lobby_session(&mut cs);

        // Toggle to hide: no other players, so no remove-entity packets expected.
        let batch = handle_visibility_toggle(&mut cs, &server).unwrap();
        let packets = batch.into_vec();
        let has_remove = packets.iter().any(|p| {
            matches!(
                p,
                PacketRegistry::RemoveEntities(_) | PacketRegistry::DestroyEntities(_)
            )
        });
        assert!(!has_remove, "no other players to remove");
    }
}
