use crate::play::data::slot_data::LobbySlot;
use minecraft_protocol::prelude::*;

/// Clientbound Set Container Slot — places an item into a specific slot of a
/// container window.  Window ID 0 is the player inventory; hotbar slot *n*
/// (0–8) maps to container slot 36+*n*.
///
/// Wire format by version:
/// - Pre-1.9:  `Byte window_id, Short slot, Slot`
/// - 1.9–1.16: `VarInt window_id, Short slot, Slot`
/// - 1.17:    `VarInt window_id, Short slot, Slot`
/// - 1.17.1+: `VarInt window_id, VarInt state_id, Short slot, Slot`
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

        if version.is_after_inclusive(ProtocolVersion::V1_17_1) {
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
    fn pre_1_9_uses_byte_window_id_no_state_id() {
        let bytes = encode(
            SetContainerSlotPacket::hotbar(0, LobbySlot::empty()),
            ProtocolVersion::V1_8,
        );
        // Byte window_id, Short slot(36), empty slot (-1 as Short)
        assert_eq!(&bytes[..3], &[0x00, 0x00, 0x24]);
        assert_eq!(bytes.len(), 5);
    }

    #[test]
    fn state_id_absent_before_v1_17_1_present_after() {
        // V1_17: no state_id — bytes are window_id, slot_hi, slot_lo(0x28=40), empty
        let bytes = encode(
            SetContainerSlotPacket::hotbar(4, LobbySlot::empty()),
            ProtocolVersion::V1_17,
        );
        assert_eq!(&bytes[..3], &[0x00, 0x00, 0x28]);
        assert_eq!(bytes.len(), 4);

        // V1_17_1 and later: state_id VarInt inserted after window_id
        for version in [ProtocolVersion::V1_17_1, ProtocolVersion::V1_20_5] {
            let bytes = encode(
                SetContainerSlotPacket::hotbar(4, LobbySlot::empty()),
                version,
            );
            assert_eq!(
                &bytes[..4],
                &[0x00, 0x00, 0x00, 0x28],
                "state_id missing for {version:?}"
            );
        }
    }

    #[test]
    fn hotbar_slot_maps_to_container_slot_36_plus_n() {
        assert_eq!(SetContainerSlotPacket::container_slot(0), 36);
        assert_eq!(SetContainerSlotPacket::container_slot(4), 40);
        assert_eq!(SetContainerSlotPacket::container_slot(8), 44);
    }
}
