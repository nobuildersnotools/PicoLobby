use crate::play::data::slot_data::LobbySlot;
use minecraft_protocol::prelude::*;

/// Clientbound Set Container Content — sends the full contents of an open
/// container window to the client.
///
/// Wire format by version:
/// - Pre-1.17.1: `UByte window_id, Short count, Slot[count]`
/// - 1.17.1+: `VarInt window_id, VarInt state_id, VarInt count, Slot[count], Slot cursor`
pub struct SetContainerContentPacket {
    window_id: u8,
    state_id: i32,
    slots: Vec<LobbySlot>,
}

impl SetContainerContentPacket {
    pub fn new(window_id: u8, state_id: i32, slots: Vec<LobbySlot>) -> Self {
        Self {
            window_id,
            state_id,
            slots,
        }
    }
}

impl EncodePacket for SetContainerContentPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if version.is_after_inclusive(ProtocolVersion::V1_17_1) {
            VarInt::new(i32::from(self.window_id)).encode(writer, version)?;
            VarInt::new(self.state_id).encode(writer, version)?;
            VarInt::new(self.slots.len() as i32).encode(writer, version)?;
            for slot in &self.slots {
                slot.encode(writer, version)?;
            }
            LobbySlot::empty().encode(writer, version)?; // cursor item, always empty
        } else {
            self.window_id.encode(writer, version)?;
            (self.slots.len() as i16).encode(writer, version)?;
            for slot in &self.slots {
                slot.encode(writer, version)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_1_17_1_uses_ubyte_window_id_and_short_count() {
        let pkt = SetContainerContentPacket::new(1, 0, vec![LobbySlot::empty()]);
        let mut writer = BinaryWriter::default();
        pkt.encode(&mut writer, ProtocolVersion::V1_17).unwrap();
        let bytes = writer.as_slice().to_vec();
        // [0x01 (window_id), 0x00 0x01 (count Short = 1), empty_slot]
        assert_eq!(bytes[0], 0x01); // window_id as UByte
        assert_eq!(bytes[1], 0x00); // short count high byte
        assert_eq!(bytes[2], 0x01); // short count low byte
    }

    #[test]
    fn v1_17_1_uses_varint_window_id_and_state_id() {
        let pkt = SetContainerContentPacket::new(1, 5, vec![LobbySlot::empty()]);
        let mut writer = BinaryWriter::default();
        pkt.encode(&mut writer, ProtocolVersion::V1_17_1).unwrap();
        let bytes = writer.as_slice().to_vec();
        // [VarInt(1) window_id, VarInt(5) state_id, VarInt(1) count, slot, cursor]
        assert_eq!(bytes[0], 0x01); // window_id
        assert_eq!(bytes[1], 0x05); // state_id
        assert_eq!(bytes[2], 0x01); // count
    }

    #[test]
    fn v1_8_uses_ubyte_window_id() {
        let pkt = SetContainerContentPacket::new(2, 0, vec![]);
        let mut writer = BinaryWriter::default();
        pkt.encode(&mut writer, ProtocolVersion::V1_8).unwrap();
        let bytes = writer.as_slice().to_vec();
        assert_eq!(bytes[0], 0x02); // window_id
        assert_eq!(bytes[1], 0x00); // count high
        assert_eq!(bytes[2], 0x00); // count low (0 slots)
    }
}
