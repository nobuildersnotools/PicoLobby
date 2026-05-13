use minecraft_packets::play::LobbySlot;
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
    fn compass_resolves_for_v1_21() {
        let sel = selector("minecraft:compass");
        assert!(
            sel.resolve_item_id(ProtocolVersion::V1_21).is_some(),
            "expected compass id for V1_21"
        );
    }

    #[test]
    fn compass_resolves_to_adjusted_v1_21_4_id() {
        let sel = selector("minecraft:compass");
        let id = sel.resolve_item_id(ProtocolVersion::V1_21_4);
        assert_eq!(id, Some(961), "V1_21_4 compass = 961");
    }

    #[test]
    fn compass_resolves_to_adjusted_v1_21_6_id() {
        let sel = selector("minecraft:compass");
        let id = sel.resolve_item_id(ProtocolVersion::V1_21_6);
        assert_eq!(id, Some(989), "V1_21_6 compass = 989");
    }

    #[test]
    fn compass_resolves_for_v1_16() {
        let sel = selector("minecraft:compass");
        let id = sel.resolve_item_id(ProtocolVersion::V1_16);
        assert_eq!(id, Some(683), "V1_16 compass = 683");
    }

    #[test]
    fn compass_resolves_for_missing_registry_versions() {
        let sel = selector("minecraft:compass");
        let cases = [
            (ProtocolVersion::V1_13, 562),
            (ProtocolVersion::V1_13_2, 567),
            (ProtocolVersion::V1_14, 621),
            (ProtocolVersion::V1_15_2, 621),
            (ProtocolVersion::V1_19_3, 861),
            (ProtocolVersion::V1_19_4, 884),
            (ProtocolVersion::V1_20_2, 888),
            (ProtocolVersion::V1_20_3, 925),
            (ProtocolVersion::V1_20_5, 928),
        ];

        for (version, expected) in cases {
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
    fn compass_resolves_for_pre_1_13() {
        let sel = selector("minecraft:compass");
        let id = sel.resolve_item_id(ProtocolVersion::V1_12_2);
        assert_eq!(id, Some(345), "pre-1.13 compass = 345");
    }

    #[test]
    fn unknown_item_returns_none() {
        let sel = selector("minecraft:unknown_item_xyz");
        assert!(sel.resolve_item_id(ProtocolVersion::V1_21).is_none());
    }

    #[test]
    fn hotbar_packet_is_none_for_unknown_item() {
        let sel = selector("minecraft:unknown_item_xyz");
        assert!(sel.build_hotbar_packet(ProtocolVersion::V1_21).is_none());
    }

    #[test]
    fn hotbar_packet_is_some_for_compass() {
        let sel = selector("minecraft:compass");
        assert!(sel.build_hotbar_packet(ProtocolVersion::V1_21).is_some());
    }
}
