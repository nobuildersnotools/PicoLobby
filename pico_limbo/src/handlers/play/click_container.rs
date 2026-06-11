use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::lobby_chat::plain_chat_feedback_packet;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{MENU_SIZE, SelectorClick, ServerState};
use minecraft_packets::play::LobbySlot;
use minecraft_packets::play::click_container_packet::ClickContainerPacket;
use minecraft_packets::play::client_bound_plugin_message_packet::PlayClientBoundPluginMessagePacket;
use minecraft_packets::play::close_container_packet::CloseContainerPacket;
use minecraft_packets::play::confirm_transaction_packet::ConfirmTransactionPacket;
use minecraft_packets::play::set_container_content_packet::SetContainerContentPacket;
use minecraft_packets::play::set_container_slot_packet::SetContainerSlotPacket;
use minecraft_protocol::prelude::ProtocolVersion;
use tracing::{info, warn};

impl PacketHandler for ClickContainerPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        let affects_lobby_hotbar_item =
            click_affects_lobby_hotbar_item(self, client_state, server_state);

        let Some(selector) = client_state.open_selector() else {
            return Ok(reject_lobby_hotbar_item_move(
                self,
                client_state,
                server_state,
                affects_lobby_hotbar_item,
            ));
        };

        if selector.window_id != self.window_id {
            return Ok(reject_lobby_hotbar_item_move(
                self,
                client_state,
                server_state,
                affects_lobby_hotbar_item,
            ));
        }

        let version = client_state.protocol_version();
        let click = selector.classify(self, version);

        match click {
            SelectorClick::Select { slot_index } => {
                // SAFETY: slot_map has exactly 27 entries; slot_index is 0..26.
                let destination_id = client_state
                    .open_selector()
                    .and_then(|s| s.slot_map[slot_index].clone());

                let Some(destination_id) = destination_id else {
                    let mut batch = resync(client_state, server_state, self);
                    resync_lobby_inventory(
                        client_state,
                        server_state,
                        affects_lobby_hotbar_item,
                        &mut batch,
                    );
                    return Ok(batch);
                };

                match server_state.resolve_lobby_destination(&destination_id) {
                    Ok(dest) => {
                        info!(
                            "Sending {} to Velocity server '{}' via selector",
                            client_state.get_username(),
                            dest.server,
                        );
                        let window_id = client_state
                            .take_open_selector()
                            .map_or(self.window_id, |s| s.window_id);

                        let mut batch = Batch::new();
                        let packet =
                            PlayClientBoundPluginMessagePacket::bungeecord_connect(&dest.server);
                        if let Some(confirm) = legacy_reject_click(version, self) {
                            batch.queue(|| PacketRegistry::ClientBoundConfirmTransaction(confirm));
                        }
                        batch.queue(|| PacketRegistry::PlayClientBoundPluginMessage(packet));
                        batch.queue(move || {
                            PacketRegistry::ClientBoundCloseContainer(CloseContainerPacket::new(
                                window_id,
                            ))
                        });
                        Ok(batch)
                    }
                    Err(err) => {
                        warn!("{}: {}", client_state.get_username(), err);
                        let msg = format!("Unknown server: {destination_id}");
                        let mut batch = resync(client_state, server_state, self);
                        resync_lobby_inventory(
                            client_state,
                            server_state,
                            affects_lobby_hotbar_item,
                            &mut batch,
                        );
                        batch.queue(move || plain_chat_feedback_packet(version, &msg));
                        Ok(batch)
                    }
                }
            }
            SelectorClick::RequiresResync => {
                let mut batch = resync(client_state, server_state, self);
                resync_lobby_inventory(
                    client_state,
                    server_state,
                    affects_lobby_hotbar_item,
                    &mut batch,
                );
                Ok(batch)
            }
            SelectorClick::Ignored => Ok(Batch::new()),
        }
    }
}

impl PacketHandler for ConfirmTransactionPacket {
    fn handle(
        &self,
        _client_state: &mut ClientState,
        _server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        Ok(Batch::new())
    }
}

fn resync(
    client_state: &mut ClientState,
    server_state: &ServerState,
    click: &ClickContainerPacket,
) -> Batch<PacketRegistry> {
    let version = client_state.protocol_version();
    let players_visible = client_state.players_visible();
    let Some(selector) = client_state.open_selector_mut() else {
        return Batch::new();
    };
    selector.state_id += 1;
    // Send the full window (menu + player inventory) so that any item the
    // client predicted moving into the player-inventory portion — e.g. by
    // shift-clicking a menu item out — is cleared back to the server's view.
    let slots = full_selector_window_slots(&selector.slots, server_state, version, players_visible);
    let pkt = SetContainerContentPacket::new(selector.window_id, selector.state_id, slots);
    let mut batch = Batch::new();
    if let Some(confirm) = legacy_reject_click(version, click) {
        batch.queue(|| PacketRegistry::ClientBoundConfirmTransaction(confirm));
    }
    batch.queue(|| PacketRegistry::SetContainerContent(pkt));
    batch
}

/// Builds the full slot contents for an open selector window: the 27 menu slots
/// followed by the 36 player-inventory slots. The configured lobby hotbar items
/// are placed in their hotbar positions; every other player slot is empty. This
/// lets a resync overwrite the player-inventory portion of the window, undoing
/// client-side predictions such as shift-clicking a menu item out.
fn full_selector_window_slots(
    menu_slots: &[LobbySlot],
    server_state: &ServerState,
    version: ProtocolVersion,
    players_visible: bool,
) -> Vec<LobbySlot> {
    // generic_9x3: 27 menu slots, then 27 main-inventory slots, then 9 hotbar slots.
    let mut slots = menu_slots.to_vec();
    slots.resize(MENU_SIZE + 36, LobbySlot::empty());

    // Window hotbar slot for player hotbar index `h` is MENU_SIZE + 27 + h.
    let hotbar_base = MENU_SIZE + 27;
    let mut place = |hotbar_slot: u8, packet: SetContainerSlotPacket| {
        let index = hotbar_base + usize::from(hotbar_slot);
        if let Some(slot) = slots.get_mut(index) {
            *slot = packet.into_slot_data();
        }
    };

    if let Some(selector) = server_state.lobby_selector()
        && let Some(packet) = selector.build_hotbar_packet(version)
    {
        place(selector.hotbar_slot, packet);
    }
    if let Some(toggle) = server_state.lobby_visibility_toggle()
        && let Some(packet) = toggle.build_hotbar_packet(players_visible, version)
    {
        place(toggle.hotbar_slot, packet);
    }

    slots
}

fn reject_lobby_hotbar_item_move(
    click: &ClickContainerPacket,
    client_state: &ClientState,
    server_state: &ServerState,
    should_reject: bool,
) -> Batch<PacketRegistry> {
    let mut batch = Batch::new();
    if !should_reject {
        return batch;
    }

    if let Some(confirm) = legacy_reject_click(client_state.protocol_version(), click) {
        batch.queue(|| PacketRegistry::ClientBoundConfirmTransaction(confirm));
    }
    resync_lobby_inventory(client_state, server_state, true, &mut batch);
    batch
}

pub(super) fn resync_lobby_inventory(
    client_state: &ClientState,
    server_state: &ServerState,
    should_resync: bool,
    batch: &mut Batch<PacketRegistry>,
) {
    if !should_resync || !server_state.lobby_enabled() {
        return;
    }

    let slots = lobby_player_inventory_slots(client_state, server_state);
    if slots.iter().all(|slot| slot.count() == 0) {
        return;
    }

    batch
        .queue(|| PacketRegistry::SetContainerContent(SetContainerContentPacket::new(0, 0, slots)));
}

fn lobby_player_inventory_slots(
    client_state: &ClientState,
    server_state: &ServerState,
) -> Vec<LobbySlot> {
    let version = client_state.protocol_version();
    let mut slots = vec![LobbySlot::empty(); player_inventory_slot_count(version)];

    if let Some(selector) = server_state.lobby_selector()
        && let Some(packet) = selector.build_hotbar_packet(version)
        && let Some(slot) = hotbar_inventory_slot(selector.hotbar_slot, version)
    {
        slots[slot] = packet.into_slot_data();
    }

    if let Some(toggle) = server_state.lobby_visibility_toggle()
        && let Some(packet) = toggle.build_hotbar_packet(client_state.players_visible(), version)
        && let Some(slot) = hotbar_inventory_slot(toggle.hotbar_slot, version)
    {
        slots[slot] = packet.into_slot_data();
    }

    slots
}

fn player_inventory_slot_count(version: ProtocolVersion) -> usize {
    if version.is_before_inclusive(ProtocolVersion::V1_8) {
        45
    } else {
        46
    }
}

fn hotbar_inventory_slot(hotbar_slot: u8, version: ProtocolVersion) -> Option<usize> {
    let slot = 36 + usize::from(hotbar_slot);
    (slot < player_inventory_slot_count(version)).then_some(slot)
}

fn click_affects_lobby_hotbar_item(
    click: &ClickContainerPacket,
    client_state: &ClientState,
    server_state: &ServerState,
) -> bool {
    let protected_slots = protected_lobby_hotbar_slots(server_state);
    if protected_slots.is_empty() {
        return false;
    }

    if click.mode == 2 && protected_slots.contains(&click.button) {
        return true;
    }

    if matches!(click.mode, 5 | 6) {
        return is_protected_inventory_context(click, client_state);
    }

    protected_slots.iter().any(|slot| {
        protected_container_slot(click, client_state, *slot).is_some_and(|s| click.slot == s)
    })
}

pub(super) fn selected_hotbar_slot_is_lobby_item(
    client_state: &ClientState,
    server_state: &ServerState,
) -> bool {
    protected_lobby_hotbar_slots(server_state).contains(&client_state.selected_hotbar_slot())
}

fn protected_lobby_hotbar_slots(server_state: &ServerState) -> Vec<u8> {
    if !server_state.lobby_enabled() {
        return Vec::new();
    }

    let mut slots = Vec::with_capacity(2);
    if let Some(selector) = server_state.lobby_selector() {
        slots.push(selector.hotbar_slot);
    }
    if let Some(toggle) = server_state.lobby_visibility_toggle() {
        slots.push(toggle.hotbar_slot);
    }
    slots
}

fn is_protected_inventory_context(
    click: &ClickContainerPacket,
    client_state: &ClientState,
) -> bool {
    click.window_id == 0
        || client_state
            .open_selector()
            .is_some_and(|selector| selector.window_id == click.window_id)
}

fn protected_container_slot(
    click: &ClickContainerPacket,
    client_state: &ClientState,
    hotbar_slot: u8,
) -> Option<i16> {
    if click.window_id == 0 {
        Some(36 + i16::from(hotbar_slot))
    } else if client_state
        .open_selector()
        .is_some_and(|selector| selector.window_id == click.window_id)
    {
        Some(54 + i16::from(hotbar_slot))
    } else {
        None
    }
}

fn legacy_reject_click(
    version: ProtocolVersion,
    click: &ClickContainerPacket,
) -> Option<ConfirmTransactionPacket> {
    version
        .is_before_inclusive(ProtocolVersion::V1_12_2)
        .then(|| ConfirmTransactionPacket::new(click.window_id, click.action_number, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::lobby::{SelectorItemConfig, VisibilityToggleConfig};
    use crate::server_state::{LobbyDestination, build_selector_menu};

    fn click(
        window_id: u8,
        slot: i16,
        button: u8,
        mode: u8,
        version: ProtocolVersion,
    ) -> ClickContainerPacket {
        ClickContainerPacket {
            window_id,
            state_id: i32::from(version.is_after_inclusive(ProtocolVersion::V1_17_1)),
            slot,
            action_number: 7,
            button,
            mode,
        }
    }

    fn client(version: ProtocolVersion) -> ClientState {
        let mut client = ClientState::default();
        client.set_protocol_version(version);
        client
    }

    fn server_with_lobby_items() -> ServerState {
        let mut builder = ServerState::builder();
        builder
            .set_lobby_enabled(true)
            .set_lobby_selector(Some(SelectorItemConfig {
                slot: 4,
                item: "minecraft:compass".to_string(),
                display_name: None,
                lore: vec![],
                filler: None,
            }))
            .unwrap()
            .set_lobby_visibility_toggle(Some(VisibilityToggleConfig {
                slot: 8,
                item: "minecraft:ender_eye".to_string(),
                display_name_on: None,
                display_name_off: None,
                lore_on: vec![],
                lore_off: vec![],
                message_on: None,
                message_off: None,
            }))
            .unwrap();
        builder.build().unwrap()
    }

    fn count_set_contents(packets: &[PacketRegistry]) -> usize {
        packets
            .iter()
            .filter(|packet| matches!(packet, PacketRegistry::SetContainerContent(_)))
            .count()
    }

    #[test]
    fn protected_player_inventory_hotbar_click_resyncs_inventory() {
        let server = server_with_lobby_items();
        let mut client = client(ProtocolVersion::V1_21);
        let click = click(0, 40, 0, 0, ProtocolVersion::V1_21);

        let packets = click.handle(&mut client, &server).unwrap().into_vec();

        assert_eq!(count_set_contents(&packets), 1);
        assert!(
            !packets
                .iter()
                .any(|packet| matches!(packet, PacketRegistry::ClientBoundConfirmTransaction(_)))
        );
    }

    #[test]
    fn shift_clicking_protected_hotbar_slot_resyncs_inventory() {
        let server = server_with_lobby_items();
        let mut client = client(ProtocolVersion::V1_21);
        let click = click(0, 40, 0, 1, ProtocolVersion::V1_21);

        let packets = click.handle(&mut client, &server).unwrap().into_vec();

        assert_eq!(count_set_contents(&packets), 1);
    }

    #[test]
    fn dragging_in_player_inventory_resyncs_protected_inventory() {
        let server = server_with_lobby_items();
        let mut client = client(ProtocolVersion::V1_21);
        let click = click(0, -999, 0, 5, ProtocolVersion::V1_21);

        let packets = click.handle(&mut client, &server).unwrap().into_vec();

        assert_eq!(count_set_contents(&packets), 1);
    }

    #[test]
    fn number_key_swap_with_protected_hotbar_slot_resyncs_inventory() {
        let server = server_with_lobby_items();
        let mut client = client(ProtocolVersion::V1_21);
        let click = click(0, 10, 4, 2, ProtocolVersion::V1_21);

        let packets = click.handle(&mut client, &server).unwrap().into_vec();

        assert_eq!(count_set_contents(&packets), 1);
    }

    #[test]
    fn unrelated_player_inventory_click_is_ignored() {
        let server = server_with_lobby_items();
        let mut client = client(ProtocolVersion::V1_21);
        let click = click(0, 37, 0, 0, ProtocolVersion::V1_21);

        let packets = click.handle(&mut client, &server).unwrap().into_vec();

        assert!(packets.is_empty());
    }

    #[test]
    fn legacy_protected_hotbar_click_rejects_transaction_and_resyncs_inventory() {
        let server = server_with_lobby_items();
        let mut client = client(ProtocolVersion::V1_12_2);
        let click = click(0, 44, 0, 0, ProtocolVersion::V1_12_2);

        let packets = click.handle(&mut client, &server).unwrap().into_vec();

        assert_eq!(count_set_contents(&packets), 1);
        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, PacketRegistry::ClientBoundConfirmTransaction(_)))
        );
    }

    #[test]
    fn full_selector_window_includes_player_inventory_and_hotbar_items() {
        let server = server_with_lobby_items();
        let menu = vec![LobbySlot::empty(); MENU_SIZE];

        let slots = full_selector_window_slots(&menu, &server, ProtocolVersion::V1_21, true);

        // 27 menu slots + 36 player-inventory slots make up the generic_9x3 window.
        assert_eq!(slots.len(), MENU_SIZE + 36);
        // The configured selector (hotbar 4) and toggle (hotbar 8) appear in the
        // window's hotbar region (base MENU_SIZE + 27).
        assert_ne!(slots[MENU_SIZE + 27 + 4].item_id(), -1);
        assert_ne!(slots[MENU_SIZE + 27 + 8].item_id(), -1);
        // The main-inventory region carries no items, so a shift-clicked ghost
        // item landing there is cleared on resync.
        for slot in &slots[MENU_SIZE..MENU_SIZE + 27] {
            assert_eq!(slot.item_id(), -1);
        }
    }

    #[test]
    fn shift_clicking_menu_item_resyncs_full_selector_window() {
        let server = server_with_lobby_items();
        let mut client = client(ProtocolVersion::V1_21);
        let destination = LobbyDestination::new("survival", "Survival", "survival");
        client.set_open_selector(build_selector_menu(
            1,
            &[destination],
            None,
            ProtocolVersion::V1_21,
        ));
        // Shift-click (mode 1) the menu slot holding the destination.
        let click = click(1, 0, 0, 1, ProtocolVersion::V1_21);

        let packets = click.handle(&mut client, &server).unwrap().into_vec();

        // The selector window is resynced rather than letting the item leave it.
        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, PacketRegistry::SetContainerContent(_)))
        );
    }

    #[test]
    fn protected_hotbar_click_below_open_selector_resyncs_menu_and_hotbar_items() {
        let server = server_with_lobby_items();
        let mut client = client(ProtocolVersion::V1_21);
        let destination = LobbyDestination::new("survival", "Survival", "survival");
        client.set_open_selector(build_selector_menu(
            1,
            &[destination],
            None,
            ProtocolVersion::V1_21,
        ));
        let click = click(1, 58, 0, 0, ProtocolVersion::V1_21);

        let packets = click.handle(&mut client, &server).unwrap().into_vec();

        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, PacketRegistry::SetContainerContent(_)))
        );
        assert_eq!(count_set_contents(&packets), 2);
    }
}
