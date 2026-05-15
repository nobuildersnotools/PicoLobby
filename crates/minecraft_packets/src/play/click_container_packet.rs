use minecraft_protocol::prelude::*;

/// Serverbound Click Container — sent when the player clicks a slot in an
/// open container window.
///
/// Wire format by version:
/// - Pre-1.17: `Byte window_id, Short slot, Byte button, Short action_number, Byte mode, Slot`
/// - 1.17+: `VarInt window_id, VarInt state_id, Short slot, Byte button, VarInt mode,
///           VarInt num_changed, (Short, Slot)[], Slot cursor`
///
/// Only the fields needed for click classification are decoded; trailing slot
/// data is left unread and discarded by the packet framing layer.
pub struct ClickContainerPacket {
    pub window_id: u8,
    /// Container state counter; 0 for pre-1.17 where the field does not exist.
    pub state_id: i32,
    /// Slot index clicked.  -999 means outside the window.
    pub slot: i16,
    /// Legacy transaction/action number used by pre-1.17 clients.
    pub action_number: i16,
    /// 0 = left click, 1 = right click.
    pub button: u8,
    /// Interaction mode: 0 = normal, 1 = shift, 2 = number key, 3 = middle,
    /// 4 = drop, 5 = drag, 6 = double-click.
    pub mode: u8,
}

impl DecodePacket for ClickContainerPacket {
    fn decode(
        reader: &mut BinaryReader,
        version: ProtocolVersion,
    ) -> Result<Self, BinaryReaderError> {
        if version.is_after_inclusive(ProtocolVersion::V1_17) {
            let window_id = VarInt::decode(reader, version)?.inner() as u8;
            let state_id = VarInt::decode(reader, version)?.inner();
            let slot = i16::decode(reader, version)?;
            let button = u8::decode(reader, version)?;
            let mode = VarInt::decode(reader, version)?.inner() as u8;
            // Remaining fields (changed_slots array, cursor_item) are not needed.
            Ok(Self {
                window_id,
                state_id,
                slot,
                action_number: 0,
                button,
                mode,
            })
        } else {
            let window_id = u8::decode(reader, version)?;
            let slot = i16::decode(reader, version)?;
            let button = u8::decode(reader, version)?;
            let _action_number = i16::decode(reader, version)?;
            let mode = u8::decode(reader, version)?;
            // Slot payload follows but is not needed.
            Ok(Self {
                window_id,
                state_id: 0,
                slot,
                action_number: _action_number,
                button,
                mode,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_modern_left_click_on_slot_3() {
        // VarInt(1) window_id, VarInt(2) state_id, Short(3) slot, Byte(0) button, VarInt(0) mode
        let bytes = [0x01, 0x02, 0x00, 0x03, 0x00, 0x00];
        let mut reader = BinaryReader::new(&bytes);
        let pkt = ClickContainerPacket::decode(&mut reader, ProtocolVersion::V1_20_5).unwrap();
        assert_eq!(pkt.window_id, 1);
        assert_eq!(pkt.state_id, 2);
        assert_eq!(pkt.slot, 3);
        assert_eq!(pkt.action_number, 0);
        assert_eq!(pkt.button, 0);
        assert_eq!(pkt.mode, 0);
    }

    #[test]
    fn decodes_legacy_right_click_on_slot_5() {
        // Byte(1) window_id, Short(5) slot, Byte(1) button, Short(0) action_num, Byte(0) mode
        let bytes = [0x01, 0x00, 0x05, 0x01, 0x00, 0x00, 0x00];
        let mut reader = BinaryReader::new(&bytes);
        let pkt = ClickContainerPacket::decode(&mut reader, ProtocolVersion::V1_12_2).unwrap();
        assert_eq!(pkt.window_id, 1);
        assert_eq!(pkt.state_id, 0);
        assert_eq!(pkt.slot, 5);
        assert_eq!(pkt.action_number, 0);
        assert_eq!(pkt.button, 1);
        assert_eq!(pkt.mode, 0);
    }

    #[test]
    fn decodes_outside_window_click() {
        // slot = -999 in two's-complement i16 big-endian = 0xFC19
        // Pre-1.17: Byte(1) window_id, Short(-999) slot, Byte(0) button, Short(0) action, Byte(0) mode
        let bytes = [0x01, 0xFC, 0x19, 0x00, 0x00, 0x00, 0x00];
        let mut reader = BinaryReader::new(&bytes);
        let pkt = ClickContainerPacket::decode(&mut reader, ProtocolVersion::V1_8).unwrap();
        assert_eq!(pkt.slot, -999);
    }
}
