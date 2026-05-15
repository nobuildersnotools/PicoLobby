use crate::server_state::navigation::LobbyDestination;
use minecraft_packets::play::LobbySlot;
use minecraft_packets::play::click_container_packet::ClickContainerPacket;
use minecraft_packets::play::set_container_slot_packet::SetContainerSlotPacket;
use minecraft_protocol::prelude::ProtocolVersion;
use pico_precomputed_registries::PrecomputedRegistries;
use pico_text_component::prelude::{Component, MiniMessageError, parse_mini_message};

/// Configured hotbar selector item.
#[derive(Debug, Clone)]
pub struct LobbySelector {
    pub hotbar_slot: u8,
    item_identifier: String,
    display_name: Option<Component>,
    lore: Vec<Component>,
    /// Legacy pre-1.13 numeric item ID, if the item existed under that scheme.
    legacy_item_id: Option<i32>,
}

impl LobbySelector {
    /// Build and validate a `LobbySelector`.
    ///
    /// `item_identifier` must be a full Minecraft identifier such as
    /// `"minecraft:compass"`.  Item IDs are resolved from the precomputed
    /// registry for each supported version bucket.  Pre-1.13 versions rely on a
    /// small hardcoded table.
    ///
    /// # Errors
    /// Returns an error if any `MiniMessage` string fails to parse.
    pub fn new(
        hotbar_slot: u8,
        item_identifier: impl Into<String>,
        display_name_mm: Option<&str>,
        lore_mm: &[String],
    ) -> Result<Self, MiniMessageError> {
        let item_identifier = item_identifier.into();

        let display_name = display_name_mm.map(parse_mini_message).transpose()?;
        let lore: Vec<Component> = lore_mm
            .iter()
            .map(|s| parse_mini_message(s))
            .collect::<Result<_, _>>()?;

        let legacy_item_id = legacy_item_id_for(&item_identifier);

        Ok(Self {
            hotbar_slot,
            item_identifier,
            display_name,
            lore,
            legacy_item_id,
        })
    }

    pub const fn display_name(&self) -> Option<&Component> {
        self.display_name.as_ref()
    }

    /// Builds a `SetContainerSlotPacket` for the player's hotbar using the
    /// item ID appropriate for `version`.  Returns `None` if the item is
    /// unknown for that version.
    pub fn build_hotbar_packet(&self, version: ProtocolVersion) -> Option<SetContainerSlotPacket> {
        let item_id = self.resolve_item_id(version)?;
        let slot = LobbySlot::new(item_id, 1, self.display_name.clone(), self.lore.clone());
        Some(SetContainerSlotPacket::hotbar(self.hotbar_slot, slot))
    }

    /// Returns the protocol item ID for `version`, or `None` if unknown.
    pub fn resolve_item_id(&self, version: ProtocolVersion) -> Option<i32> {
        if version.is_before_inclusive(ProtocolVersion::V1_12_2) {
            return self.legacy_item_id;
        }
        PrecomputedRegistries::new(version).resolve_item_id(&self.item_identifier)
    }
}

/// Per-player state stored in `ClientState` while a selector GUI is open.
pub struct OpenSelectorState {
    pub window_id: u8,
    /// Container state counter.  Incremented each time `SetContainerContent` is
    /// sent (including resyncs).  Only meaningful for 1.17.1+ clients.
    pub state_id: i32,
    /// 27-element vec; index = container slot.  `Some(id)` means that slot
    /// holds a navigation destination with that id.
    pub slot_map: Vec<Option<String>>,
    /// Pre-built, version-resolved item stacks for the 27 menu slots.
    pub slots: Vec<LobbySlot>,
}

/// Result of classifying a `ClickContainerPacket` against an open selector.
pub enum SelectorClick {
    /// Left or right click on a slot that holds a destination.
    Select { slot_index: usize },
    /// Invalid or unsupported interaction; the client should be resynced.
    RequiresResync,
    /// Click outside the window (-999) — no state change needed.
    Ignored,
}

impl OpenSelectorState {
    /// Classify a click packet into a `SelectorClick`.
    pub fn classify(
        &self,
        packet: &ClickContainerPacket,
        version: ProtocolVersion,
    ) -> SelectorClick {
        // Click outside window
        if packet.slot == -999 {
            return SelectorClick::Ignored;
        }
        // Shift, number-key, drag, etc.
        if packet.mode != 0 {
            return SelectorClick::RequiresResync;
        }
        // Out of menu range (player inventory area below slot 26)
        if packet.slot < 0 || packet.slot >= 27 {
            return SelectorClick::RequiresResync;
        }
        // Stale state_id check (1.17.1+ only)
        if version.is_after_inclusive(ProtocolVersion::V1_17_1) && packet.state_id != self.state_id
        {
            return SelectorClick::RequiresResync;
        }
        // slot is already verified >= 0 above.
        let slot_index = packet.slot.cast_unsigned() as usize;
        if self.slot_map[slot_index].is_some() {
            SelectorClick::Select { slot_index }
        } else {
            SelectorClick::RequiresResync
        }
    }
}

const MENU_SIZE: usize = 27;

/// Builds the initial `OpenSelectorState` for a player opening the selector
/// menu.  Destinations fill slots 0, 1, 2, … up to 27; remaining slots are
/// empty.  All item IDs are resolved for `version`.
pub fn build_selector_menu(
    window_id: u8,
    destinations: &[LobbyDestination],
    version: ProtocolVersion,
) -> OpenSelectorState {
    let paper_id = resolve_paper_id(version);

    let mut slots = Vec::with_capacity(MENU_SIZE);
    let mut slot_map = vec![None; MENU_SIZE];

    for (i, dest) in destinations.iter().take(MENU_SIZE).enumerate() {
        let display = parse_mini_message(&dest.display_name).ok();
        let lore = parse_mini_message("<gray>Click to connect.")
            .ok()
            .into_iter()
            .collect();
        slots.push(LobbySlot::new(paper_id, 1, display, lore));
        slot_map[i] = Some(dest.id.0.clone());
    }

    while slots.len() < MENU_SIZE {
        slots.push(LobbySlot::empty());
    }

    OpenSelectorState {
        window_id,
        state_id: 1,
        slot_map,
        slots,
    }
}

fn resolve_paper_id(version: ProtocolVersion) -> i32 {
    if version.is_before_inclusive(ProtocolVersion::V1_12_2) {
        339 // pre-1.13 numeric ID for paper
    } else {
        PrecomputedRegistries::new(version)
            .resolve_item_id("minecraft:paper")
            .unwrap_or(339)
    }
}

/// Hardcoded numeric item IDs for common selector items in pre-1.13 clients.
/// These are the Java item IDs from before the 1.13 flattening.
fn legacy_item_id_for(identifier: &str) -> Option<i32> {
    let id = match identifier {
        "minecraft:compass" => 345,
        "minecraft:clock" => 347,
        "minecraft:nether_star" => 399,
        "minecraft:paper" => 339,
        "minecraft:book" => 340,
        "minecraft:written_book" => 387,
        "minecraft:map" | "minecraft:filled_map" => 395,
        "minecraft:ender_pearl" => 368,
        "minecraft:diamond" => 264,
        "minecraft:emerald" => 388,
        "minecraft:gold_ingot" => 266,
        "minecraft:iron_ingot" => 265,
        _ => return None,
    };
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selector(item: &str) -> LobbySelector {
        LobbySelector::new(4, item, None, &[]).expect("valid selector")
    }

    #[test]
    fn compass_id_per_version_bucket() {
        let sel = selector("minecraft:compass");
        for (version, expected) in [
            (ProtocolVersion::V1_12_2, 345),  // pre-1.13 legacy numeric
            (ProtocolVersion::V1_13,   562),
            (ProtocolVersion::V1_13_2, 567),
            (ProtocolVersion::V1_14,   621),
            (ProtocolVersion::V1_15_2, 621),
            (ProtocolVersion::V1_16,   683),
            (ProtocolVersion::V1_19_3, 861),
            (ProtocolVersion::V1_19_4, 884),
            (ProtocolVersion::V1_20_2, 888),
            (ProtocolVersion::V1_20_3, 925),
            (ProtocolVersion::V1_20_5, 928),
            (ProtocolVersion::V1_21,   928),
            (ProtocolVersion::V1_21_4, 961),
            (ProtocolVersion::V1_21_6, 989),
        ] {
            assert_eq!(
                sel.resolve_item_id(version),
                Some(expected),
                "{version:?} compass = {expected}"
            );
        }
    }

    #[test]
    fn item_absent_from_missing_registry_version_returns_none() {
        let sel = selector("minecraft:netherite_chestplate");
        assert_eq!(sel.resolve_item_id(ProtocolVersion::V1_15_2), None);
    }

    #[test]
    fn hotbar_packet_reflects_item_resolution() {
        let unknown = selector("minecraft:unknown_item_xyz");
        assert!(unknown.resolve_item_id(ProtocolVersion::V1_21).is_none());
        assert!(unknown.build_hotbar_packet(ProtocolVersion::V1_21).is_none());

        let compass = selector("minecraft:compass");
        assert!(compass.build_hotbar_packet(ProtocolVersion::V1_21).is_some());
    }

    // ── selector menu tests ───────────────────────────────────────────────────

    fn dest(id: &str) -> LobbyDestination {
        LobbyDestination::new(id, id, "lobby")
    }

    #[test]
    fn build_selector_menu_slot_layout() {
        let dests = vec![dest("survival"), dest("creative"), dest("minigames")];
        let state = build_selector_menu(1, &dests, ProtocolVersion::V1_21);

        assert_eq!(state.slot_map[0], Some("survival".to_string()));
        assert_eq!(state.slot_map[1], Some("creative".to_string()));
        assert_eq!(state.slot_map[2], Some("minigames".to_string()));
        assert_eq!(state.slots.len(), 27);
        for i in 3..27 {
            assert!(state.slot_map[i].is_none());
            assert_eq!(state.slots[i].item_id(), -1);
        }
    }

    fn make_click(slot: i16, button: u8, mode: u8, state_id: i32) -> ClickContainerPacket {
        ClickContainerPacket {
            window_id: 1,
            state_id,
            slot,
            action_number: 0,
            button,
            mode,
        }
    }

    fn open_state_with_two_dests() -> OpenSelectorState {
        build_selector_menu(1, &[dest("a"), dest("b")], ProtocolVersion::V1_21)
    }

    #[test]
    fn classify_clicks_select_destination_slot() {
        let state = open_state_with_two_dests();
        assert!(matches!(
            state.classify(&make_click(0, 0, 0, 1), ProtocolVersion::V1_21),
            SelectorClick::Select { slot_index: 0 }
        ));
        assert!(matches!(
            state.classify(&make_click(1, 1, 0, 1), ProtocolVersion::V1_21),
            SelectorClick::Select { slot_index: 1 }
        ));
    }

    #[test]
    fn classify_invalid_interactions_require_resync() {
        let state = open_state_with_two_dests();
        let version = ProtocolVersion::V1_21;
        // shift-click (mode 1), drag (mode 5), out-of-range slot, empty slot
        for pkt in [
            make_click(0, 0, 1, 1),   // shift
            make_click(0, 0, 5, 1),   // drag
            make_click(27, 0, 0, 1),  // player inventory slot
            make_click(5, 0, 0, 1),   // empty menu slot
        ] {
            assert!(
                matches!(state.classify(&pkt, version), SelectorClick::RequiresResync),
                "expected RequiresResync for pkt slot={} mode={}",
                pkt.slot,
                pkt.mode
            );
        }
    }

    #[test]
    fn classify_outside_window_is_ignored() {
        let state = open_state_with_two_dests();
        assert!(matches!(
            state.classify(&make_click(-999, 0, 0, 1), ProtocolVersion::V1_21),
            SelectorClick::Ignored
        ));
    }

    #[test]
    fn classify_stale_state_id_version_behavior() {
        let state = open_state_with_two_dests(); // state_id = 1
        let stale = make_click(0, 0, 0, 0);     // state_id = 0

        // 1.17.1+: stale state_id → resync
        assert!(matches!(
            state.classify(&stale, ProtocolVersion::V1_17_1),
            SelectorClick::RequiresResync
        ));
        // pre-1.17.1: state_id ignored → select
        assert!(matches!(
            state.classify(&stale, ProtocolVersion::V1_12_2),
            SelectorClick::Select { slot_index: 0 }
        ));
    }
}
