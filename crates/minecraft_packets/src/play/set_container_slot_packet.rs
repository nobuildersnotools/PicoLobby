use crate::play::data::slot_data::LobbySlot;
use minecraft_protocol::prelude::*;

/// Clientbound Set Container Slot — places an item into a specific slot of a
/// container window.  Window ID 0 is the player inventory; hotbar slot *n*
/// (0–8) maps to container slot 36+*n*.
///
/// Wire format by version:
/// - Pre-1.9:  `Byte window_id, Short slot, Slot`
/// - 1.9–1.16: `VarInt window_id, Short slot, Slot`
/// - 1.17+:   `VarInt window_id, VarInt state_id, Short slot, Slot`
pub struct SetContainerSlotPacket {
    window_id: i8,
    hotbar_slot: u8,
    slot_data: LobbySlot,
}

impl SetContainerSlotPacket {
    /// Creates a packet that places `slot_data` into `hotbar_slot` (0–8) of
    /// the player's own inventory (window 0).
    pub fn hotbar(hotbar_slot: u8, slot_data: LobbySlot) -> Self {
        Self {
            window_id: 0,
            hotbar_slot,
            slot_data,
        }
    }

    /// Container slot index for a hotbar slot 0–8 in the player inventory.
    const fn container_slot(hotbar_slot: u8) -> i16 {
        36 + hotbar_slot as i16
    }
}

impl EncodePacket for SetContainerSlotPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if version.is_before_inclusive(ProtocolVersion::V1_8) {
            // Pre-1.9: window_id as Byte
            self.window_id.encode(writer, version)?;
        } else {
            // 1.9+: window_id as VarInt
            VarInt::new(i32::from(self.window_id)).encode(writer, version)?;
        }

        if version.is_after_inclusive(ProtocolVersion::V1_17) {
            // state_id — always 0; we do not track container state
            VarInt::new(0).encode(writer, version)?;
        }

        Self::container_slot(self.hotbar_slot).encode(writer, version)?;
        self.slot_data.encode(writer, version)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(pkt: SetContainerSlotPacket, version: ProtocolVersion) -> Vec<u8> {
        let mut writer = BinaryWriter::default();
        pkt.encode(&mut writer, version).expect("encode failed");
        writer.as_slice().to_vec()
    }

    #[test]
    fn pre_1_9_uses_byte_window_id_and_no_state_id() {
        let pkt = SetContainerSlotPacket::hotbar(0, LobbySlot::empty());
        let bytes = encode(pkt, ProtocolVersion::V1_8);
        // [window_id=0 (byte), slot=36 (short), empty_slot]
        assert_eq!(bytes[0], 0x00); // window_id as byte
        assert_eq!(bytes[1], 0x00); // slot high byte
        assert_eq!(bytes[2], 0x24); // slot low byte (36)
        // no state_id
        assert_eq!(bytes.len(), 5); // 1 + 2 + 2 (short -1 for empty)
    }

    #[test]
    fn modern_uses_varint_window_and_state_id() {
        let pkt = SetContainerSlotPacket::hotbar(0, LobbySlot::empty());
        let bytes = encode(pkt, ProtocolVersion::V1_20_5);
        assert_eq!(bytes[0], 0x00); // VarInt(0) window_id
        assert_eq!(bytes[1], 0x00); // VarInt(0) state_id
        assert_eq!(bytes[2], 0x00); // slot high byte
        assert_eq!(bytes[3], 0x24); // slot low byte (36)
        // empty slot = VarInt(0)
        assert_eq!(bytes[4], 0x00);
    }

    #[test]
    fn hotbar_slot_4_maps_to_container_slot_40() {
        assert_eq!(SetContainerSlotPacket::container_slot(4), 40);
    }
}
