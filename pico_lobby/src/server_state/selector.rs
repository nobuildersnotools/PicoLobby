use crate::configuration::lobby::VisibilityToggleConfig;
use crate::server_state::legacy_items::legacy_item;
use crate::server_state::navigation::LobbyDestination;
use minecraft_packets::play::LobbySlot;
use minecraft_packets::play::click_container_packet::ClickContainerPacket;
use minecraft_packets::play::set_container_slot_packet::SetContainerSlotPacket;
use minecraft_protocol::prelude::ProtocolVersion;
use pico_precomputed_registries::PrecomputedRegistries;
use pico_text_component::prelude::{Component, MiniMessageError, parse_mini_message};

/// A version-resolved background filler item used for the selector GUI's empty
/// slots.
#[derive(Debug, Clone)]
pub struct LobbyFiller {
    pub(crate) item_identifier: String,
    display_name: Option<Component>,
    lore: Vec<Component>,
}

impl LobbyFiller {
    /// Build and validate a `LobbyFiller` from its config strings.
    ///
    /// # Errors
    /// Returns an error if any `MiniMessage` string fails to parse.
    pub fn new(
        item_identifier: impl Into<String>,
        display_name_mm: Option<&str>,
        lore_mm: &[String],
    ) -> Result<Self, MiniMessageError> {
        let display_name = display_name_mm.map(parse_mini_message).transpose()?;
        let lore: Vec<Component> = lore_mm
            .iter()
            .map(|s| parse_mini_message(s))
            .collect::<Result<_, _>>()?;

        Ok(Self {
            item_identifier: item_identifier.into(),
            display_name,
            lore,
        })
    }

    /// Builds the version-resolved filler item stack, or `None` if the item is
    /// unknown for `version` (in which case the slot is left empty).
    fn build_slot(&self, version: ProtocolVersion) -> Option<LobbySlot> {
        let (item_id, damage) = resolve_item(&self.item_identifier, version)?;
        Some(
            LobbySlot::new(item_id, 1, self.display_name.clone(), self.lore.clone())
                .with_legacy_damage(damage),
        )
    }
}

/// Configured hotbar selector item.
#[derive(Debug, Clone)]
pub struct LobbySelector {
    pub hotbar_slot: u8,
    pub(crate) item_identifier: String,
    display_name: Option<Component>,
    lore: Vec<Component>,
    filler: Option<LobbyFiller>,
}

impl LobbySelector {
    /// Build and validate a `LobbySelector`.
    ///
    /// `item_identifier` must be a full Minecraft identifier such as
    /// `"minecraft:compass"`.  Item IDs are resolved from the precomputed
    /// registry for each supported version bucket.  Pre-1.13 versions resolve
    /// from the generated pre-Flattening table.
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

        Ok(Self {
            hotbar_slot,
            item_identifier,
            display_name,
            lore,
            filler: None,
        })
    }

    /// Attaches a background filler item used for the selector GUI's empty slots.
    #[must_use]
    pub fn with_filler(mut self, filler: Option<LobbyFiller>) -> Self {
        self.filler = filler;
        self
    }

    pub const fn filler(&self) -> Option<&LobbyFiller> {
        self.filler.as_ref()
    }

    pub const fn display_name(&self) -> Option<&Component> {
        self.display_name.as_ref()
    }

    /// Builds a `SetContainerSlotPacket` for the player's hotbar using the
    /// item ID appropriate for `version`.  Returns `None` if the item is
    /// unknown for that version.
    pub fn build_hotbar_packet(&self, version: ProtocolVersion) -> Option<SetContainerSlotPacket> {
        let (item_id, damage) = resolve_item(&self.item_identifier, version)?;
        let slot = LobbySlot::new(item_id, 1, self.display_name.clone(), self.lore.clone())
            .with_legacy_damage(damage);
        Some(SetContainerSlotPacket::hotbar(self.hotbar_slot, slot))
    }
}

/// Per-player hotbar visibility toggle item.
#[derive(Debug, Clone)]
pub struct LobbyVisibilityToggle {
    pub hotbar_slot: u8,
    pub(crate) item_identifier: String,
    display_name_on: Option<Component>,
    display_name_off: Option<Component>,
    lore_on: Vec<Component>,
    lore_off: Vec<Component>,
    pub message_on: Option<String>,
    pub message_off: Option<String>,
}

impl LobbyVisibilityToggle {
    /// Build and validate a `LobbyVisibilityToggle` from its config.
    ///
    /// # Errors
    /// Returns an error if any `MiniMessage` string fails to parse.
    pub fn new(config: VisibilityToggleConfig) -> Result<Self, MiniMessageError> {
        let item_identifier = config.item;

        let display_name_on = config
            .display_name_on
            .as_deref()
            .map(parse_mini_message)
            .transpose()?;
        let display_name_off = config
            .display_name_off
            .as_deref()
            .map(parse_mini_message)
            .transpose()?;
        let lore_on = config
            .lore_on
            .iter()
            .map(|s| parse_mini_message(s))
            .collect::<Result<_, _>>()?;
        let lore_off = config
            .lore_off
            .iter()
            .map(|s| parse_mini_message(s))
            .collect::<Result<_, _>>()?;

        Ok(Self {
            hotbar_slot: config.slot,
            item_identifier,
            display_name_on,
            display_name_off,
            lore_on,
            lore_off,
            message_on: config.message_on,
            message_off: config.message_off,
        })
    }

    /// Builds a `SetContainerSlotPacket` for the player's hotbar reflecting the
    /// current visibility state.  Returns `None` if the item is unknown for that version.
    pub fn build_hotbar_packet(
        &self,
        players_visible: bool,
        version: ProtocolVersion,
    ) -> Option<SetContainerSlotPacket> {
        let (item_id, damage) = resolve_item(&self.item_identifier, version)?;
        let (display_name, lore) = if players_visible {
            (self.display_name_on.clone(), self.lore_on.clone())
        } else {
            (self.display_name_off.clone(), self.lore_off.clone())
        };
        let slot = LobbySlot::new(item_id, 1, display_name, lore).with_legacy_damage(damage);
        Some(SetContainerSlotPacket::hotbar(self.hotbar_slot, slot))
    }

    /// Returns the feedback message for the given visibility state, if configured.
    pub fn feedback_message(&self, players_visible: bool) -> Option<&str> {
        if players_visible {
            self.message_on.as_deref()
        } else {
            self.message_off.as_deref()
        }
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

/// Number of slots in the selector GUI (a single 27-slot chest).
pub const MENU_SIZE: usize = 27;

/// Builds the initial `OpenSelectorState` for a player opening the selector
/// menu.
///
/// Each destination renders its configured item and lore. Entries with an
/// explicit `slot` are placed first; entries without one fill the remaining
/// free slots in order. Per-entry items that are unknown for `version` fall
/// back to paper so the entry still appears. Entries that cannot be placed
/// (out of range or colliding) and overflow past 27 slots are skipped.
///
/// Any slot left empty by the destinations is filled with `filler` (when
/// configured and resolvable for `version`); these filler slots are not
/// selectable.
pub fn build_selector_menu(
    window_id: u8,
    destinations: &[LobbyDestination],
    filler: Option<&LobbyFiller>,
    version: ProtocolVersion,
) -> OpenSelectorState {
    let paper_id = resolve_paper_id(version);

    let filler_slot = filler.and_then(|f| f.build_slot(version));
    let mut slots = vec![filler_slot.unwrap_or_else(LobbySlot::empty); MENU_SIZE];
    let mut slot_map: Vec<Option<String>> = vec![None; MENU_SIZE];

    // First pass: entries with an explicit, in-range, unoccupied slot.
    for dest in destinations {
        if let Some(slot) = dest.slot
            && slot < MENU_SIZE
            && slot_map[slot].is_none()
        {
            slots[slot] = build_destination_slot(dest, version, paper_id);
            slot_map[slot] = Some(dest.id.0.clone());
        }
    }

    // Second pass: auto-placed entries fill the first free slot in order.
    let mut next_free = 0usize;
    for dest in destinations {
        if dest.slot.is_some() {
            continue;
        }
        while next_free < MENU_SIZE && slot_map[next_free].is_some() {
            next_free += 1;
        }
        if next_free >= MENU_SIZE {
            break;
        }
        slots[next_free] = build_destination_slot(dest, version, paper_id);
        slot_map[next_free] = Some(dest.id.0.clone());
        next_free += 1;
    }

    OpenSelectorState {
        window_id,
        state_id: 1,
        slot_map,
        slots,
    }
}

/// Builds the version-resolved item stack for a single selector entry.
fn build_destination_slot(
    dest: &LobbyDestination,
    version: ProtocolVersion,
    paper_id: i32,
) -> LobbySlot {
    let (item_id, damage) = resolve_item(&dest.item, version).unwrap_or((paper_id, 0));
    let display = parse_mini_message(&dest.display_name).ok();
    let lore = dest
        .lore
        .iter()
        .filter_map(|line| parse_mini_message(line).ok())
        .collect();
    LobbySlot::new(item_id, 1, display, lore)
        .with_legacy_damage(damage)
        .with_glint(dest.enchanted)
}

/// Resolves the protocol item ID and legacy metadata/damage for `identifier` on
/// `version`. Pre-1.13 clients use the generated pre-Flattening table (which
/// carries a metadata value for variant items such as coloured wool); 1.13+
/// clients use the precomputed registry and always report metadata `0`.
/// Returns `None` if the item is unknown for that version.
pub fn resolve_item(identifier: &str, version: ProtocolVersion) -> Option<(i32, i16)> {
    if version.is_before_inclusive(ProtocolVersion::V1_12_2) {
        legacy_item(identifier).map(|(id, meta)| (i32::from(id), meta))
    } else {
        PrecomputedRegistries::new(version)
            .resolve_item_id(identifier)
            .map(|id| (id, 0))
    }
}

fn resolve_paper_id(version: ProtocolVersion) -> i32 {
    resolve_item("minecraft:paper", version).map_or(339, |(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selector(item: &str) -> LobbySelector {
        LobbySelector::new(4, item, None, &[]).expect("valid selector")
    }

    #[test]
    fn compass_id_per_version_bucket() {
        for (version, expected) in [
            (ProtocolVersion::V1_12_2, 345), // pre-1.13 legacy numeric
            (ProtocolVersion::V1_13, 562),
            (ProtocolVersion::V1_13_2, 567),
            (ProtocolVersion::V1_14, 621),
            (ProtocolVersion::V1_15_2, 621),
            (ProtocolVersion::V1_16, 683),
            (ProtocolVersion::V1_19_3, 861),
            (ProtocolVersion::V1_19_4, 884),
            (ProtocolVersion::V1_20_2, 888),
            (ProtocolVersion::V1_20_3, 925),
            (ProtocolVersion::V1_20_5, 928),
            (ProtocolVersion::V1_21, 928),
            (ProtocolVersion::V1_21_4, 961),
            (ProtocolVersion::V1_21_6, 989),
            (ProtocolVersion::V26_1, 1035),
            (ProtocolVersion::V26_2, 1063), // 26.2 shifted item IDs vs 26.1
        ] {
            assert_eq!(
                resolve_item("minecraft:compass", version).map(|(id, _)| id),
                Some(expected),
                "{version:?} compass = {expected}"
            );
        }
    }

    #[test]
    fn item_absent_from_missing_registry_version_returns_none() {
        assert!(resolve_item("minecraft:netherite_chestplate", ProtocolVersion::V1_15_2).is_none());
    }

    #[test]
    fn hotbar_packet_reflects_item_resolution() {
        let unknown = selector("minecraft:unknown_item_xyz");
        assert!(resolve_item("minecraft:unknown_item_xyz", ProtocolVersion::V1_21).is_none());
        assert!(
            unknown
                .build_hotbar_packet(ProtocolVersion::V1_21)
                .is_none()
        );

        let compass = selector("minecraft:compass");
        assert!(
            compass
                .build_hotbar_packet(ProtocolVersion::V1_21)
                .is_some()
        );
    }

    // ── selector menu tests ───────────────────────────────────────────────────

    fn dest(id: &str) -> LobbyDestination {
        LobbyDestination::new(id, id, "lobby")
    }

    #[test]
    fn build_selector_menu_slot_layout() {
        let dests = vec![dest("survival"), dest("creative"), dest("minigames")];
        let state = build_selector_menu(1, &dests, None, ProtocolVersion::V1_21);

        assert_eq!(state.slot_map[0], Some("survival".to_string()));
        assert_eq!(state.slot_map[1], Some("creative".to_string()));
        assert_eq!(state.slot_map[2], Some("minigames".to_string()));
        assert_eq!(state.slots.len(), 27);
        for i in 3..27 {
            assert!(state.slot_map[i].is_none());
            assert_eq!(state.slots[i].item_id(), -1);
        }
    }

    #[test]
    fn pre_1_13_item_resolves_with_legacy_id_and_metadata() {
        // diamond_pickaxe is a non-variant item: legacy id 278, metadata 0.
        assert_eq!(
            resolve_item("minecraft:diamond_pickaxe", ProtocolVersion::V1_8),
            Some((278, 0))
        );
        // red_wool is a variant item: legacy id 35, metadata 14.
        assert_eq!(
            resolve_item("minecraft:red_wool", ProtocolVersion::V1_8),
            Some((35, 14))
        );
        // 1.13+ clients resolve the flattened id and never report metadata.
        let (modern_id, modern_meta) =
            resolve_item("minecraft:red_wool", ProtocolVersion::V1_21).unwrap();
        assert_eq!(modern_meta, 0);
        assert_ne!(modern_id, 35);
    }

    #[test]
    fn explicit_slots_are_honoured_and_auto_entries_skip_them() {
        let dests = vec![
            dest("auto-a"),
            dest("pinned").with_slot(Some(0)),
            dest("auto-b"),
        ];
        let state = build_selector_menu(1, &dests, None, ProtocolVersion::V1_21);

        // The pinned entry claims slot 0; auto entries fill around it in order.
        assert_eq!(state.slot_map[0], Some("pinned".to_string()));
        assert_eq!(state.slot_map[1], Some("auto-a".to_string()));
        assert_eq!(state.slot_map[2], Some("auto-b".to_string()));
    }

    #[test]
    fn per_entry_item_overrides_default_paper() {
        let version = ProtocolVersion::V1_21;
        let (paper, _) = resolve_item("minecraft:paper", version).unwrap();
        let (compass, _) = resolve_item("minecraft:compass", version).unwrap();
        assert_ne!(paper, compass);

        let dests = vec![dest("a").with_item("minecraft:compass"), dest("b")];
        let state = build_selector_menu(1, &dests, None, version);

        assert_eq!(state.slots[0].item_id(), compass);
        assert_eq!(state.slots[1].item_id(), paper);
    }

    #[test]
    fn enchanted_entry_encodes_glint_component() {
        // An enchanted entry should attach the 1.20.5+ glint override component;
        // a plain entry should not. Compare the structured wire encodings.
        use minecraft_protocol::prelude::{BinaryWriter, EncodePacket};

        let version = ProtocolVersion::V1_21;
        let plain = build_selector_menu(1, &[dest("a")], None, version);
        let glinted = build_selector_menu(1, &[dest("a").with_enchanted(true)], None, version);

        let encode = |slot: &LobbySlot| {
            let mut writer = BinaryWriter::default();
            slot.encode(&mut writer, version).expect("encode");
            writer.as_slice().to_vec()
        };

        // The glinted slot carries one extra component (id 18 + bool true).
        assert!(encode(&glinted.slots[0]).len() > encode(&plain.slots[0]).len());
        assert!(encode(&glinted.slots[0]).ends_with(&[0x12, 0x01]));
    }

    #[test]
    fn filler_fills_empty_slots_without_making_them_selectable() {
        let version = ProtocolVersion::V1_21;
        let (pane, _) = resolve_item("minecraft:gray_stained_glass_pane", version).unwrap();
        let filler = LobbyFiller::new("minecraft:gray_stained_glass_pane", None, &[]).unwrap();

        let dests = vec![dest("survival"), dest("creative")];
        let state = build_selector_menu(1, &dests, Some(&filler), version);

        // Destination slots keep their own items and remain selectable.
        assert_eq!(state.slot_map[0], Some("survival".to_string()));
        assert_eq!(state.slot_map[1], Some("creative".to_string()));

        // Every other slot shows the filler item but holds no destination, so a
        // click there resyncs rather than navigating.
        for i in 2..MENU_SIZE {
            assert!(state.slot_map[i].is_none());
            assert_eq!(state.slots[i].item_id(), pane);
            let slot = i16::try_from(i).expect("slot index fits i16");
            assert!(matches!(
                state.classify(&make_click(slot, 0, 0, 1), version),
                SelectorClick::RequiresResync
            ));
        }
    }

    #[test]
    fn filler_legacy_item_resolves_with_metadata() {
        // gray_stained_glass_pane is a pre-Flattening variant: id 160, meta 7.
        let filler = LobbyFiller::new("minecraft:gray_stained_glass_pane", None, &[]).unwrap();
        let state = build_selector_menu(1, &[], Some(&filler), ProtocolVersion::V1_8);
        assert_eq!(state.slots[0].item_id(), 160);
    }

    #[test]
    fn unknown_filler_item_leaves_slots_empty() {
        let version = ProtocolVersion::V1_21;
        let filler = LobbyFiller::new("minecraft:not_a_real_item", None, &[]).unwrap();
        let state = build_selector_menu(1, &[], Some(&filler), version);
        assert_eq!(state.slots[0].item_id(), -1);
    }

    #[test]
    fn unknown_per_entry_item_falls_back_to_paper() {
        let version = ProtocolVersion::V1_21;
        let (paper, _) = resolve_item("minecraft:paper", version).unwrap();

        let dests = vec![dest("a").with_item("minecraft:not_a_real_item")];
        let state = build_selector_menu(1, &dests, None, version);

        assert_eq!(state.slots[0].item_id(), paper);
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
        build_selector_menu(1, &[dest("a"), dest("b")], None, ProtocolVersion::V1_21)
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
            make_click(0, 0, 1, 1),  // shift
            make_click(0, 0, 5, 1),  // drag
            make_click(27, 0, 0, 1), // player inventory slot
            make_click(5, 0, 0, 1),  // empty menu slot
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

    // ── LobbyVisibilityToggle tests ───────────────────────────────────────────

    fn visibility_toggle_config(item: &str) -> VisibilityToggleConfig {
        VisibilityToggleConfig {
            slot: 8,
            item: item.to_string(),
            display_name_on: Some("<green>Visible".to_string()),
            display_name_off: Some("<red>Hidden".to_string()),
            lore_on: vec!["<gray>Click to hide.".to_string()],
            lore_off: vec!["<gray>Click to show.".to_string()],
            message_on: Some("<green>Now visible.".to_string()),
            message_off: Some("<red>Now hidden.".to_string()),
        }
    }

    #[test]
    fn visibility_toggle_hotbar_packet_on_vs_off() {
        let toggle = LobbyVisibilityToggle::new(visibility_toggle_config("minecraft:ender_eye"))
            .expect("valid toggle");
        let on = toggle.build_hotbar_packet(true, ProtocolVersion::V1_21);
        let off = toggle.build_hotbar_packet(false, ProtocolVersion::V1_21);
        assert!(on.is_some());
        assert!(off.is_some());
    }

    #[test]
    fn visibility_toggle_feedback_message_per_state() {
        let toggle = LobbyVisibilityToggle::new(visibility_toggle_config("minecraft:ender_eye"))
            .expect("valid toggle");
        assert_eq!(toggle.feedback_message(true), Some("<green>Now visible."));
        assert_eq!(toggle.feedback_message(false), Some("<red>Now hidden."));
    }

    #[test]
    fn visibility_toggle_unknown_item_returns_none_packet() {
        let toggle = LobbyVisibilityToggle::new(visibility_toggle_config("minecraft:unknown_xyz"))
            .expect("valid toggle");
        assert!(
            toggle
                .build_hotbar_packet(true, ProtocolVersion::V1_21)
                .is_none()
        );
        assert!(
            toggle
                .build_hotbar_packet(false, ProtocolVersion::V1_21)
                .is_none()
        );
    }

    #[test]
    fn visibility_toggle_ender_eye_legacy_id() {
        assert_eq!(
            resolve_item("minecraft:ender_eye", ProtocolVersion::V1_12_2),
            Some((381, 0))
        );
        assert!(resolve_item("minecraft:ender_eye", ProtocolVersion::V1_21).is_some());
    }

    #[test]
    fn classify_stale_state_id_version_behavior() {
        let state = open_state_with_two_dests(); // state_id = 1
        let stale_click = make_click(0, 0, 0, 0); // state_id = 0

        // 1.17.1+: stale state_id → resync
        assert!(matches!(
            state.classify(&stale_click, ProtocolVersion::V1_17_1),
            SelectorClick::RequiresResync
        ));
        // pre-1.17.1: state_id ignored → select
        assert!(matches!(
            state.classify(&stale_click, ProtocolVersion::V1_12_2),
            SelectorClick::Select { slot_index: 0 }
        ));
    }
}
