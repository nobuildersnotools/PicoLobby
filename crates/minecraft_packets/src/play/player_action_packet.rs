use minecraft_protocol::prelude::*;

/// Serverbound Player Action, historically named Player Digging.
///
/// Only the action status is needed by the lobby item lock. Remaining fields
/// are left unread and discarded by the packet framing layer.
pub struct PlayerActionPacket {
    status: i32,
}

impl PlayerActionPacket {
    /// Drop entire selected stack.
    const DROP_ITEM_STACK: i32 = 3;
    /// Drop one item from the selected stack.
    const DROP_ITEM: i32 = 4;

    pub const fn status(&self) -> i32 {
        self.status
    }

    pub const fn is_drop_selected_item(&self) -> bool {
        matches!(self.status, Self::DROP_ITEM_STACK | Self::DROP_ITEM)
    }
}

impl DecodePacket for PlayerActionPacket {
    fn decode(
        reader: &mut BinaryReader,
        version: ProtocolVersion,
    ) -> Result<Self, BinaryReaderError> {
        let status = VarInt::decode(reader, version)?.inner();
        Ok(Self { status })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_drop_status_and_ignores_remaining_fields() {
        let bytes = [0x04, 0x00, 0x00, 0x00];
        let mut reader = BinaryReader::new(&bytes);

        let packet = PlayerActionPacket::decode(&mut reader, ProtocolVersion::V1_21).unwrap();

        assert_eq!(packet.status(), 4);
        assert!(packet.is_drop_selected_item());
    }

    #[test]
    fn non_drop_status_is_not_selected_item_drop() {
        let bytes = [0x00];
        let mut reader = BinaryReader::new(&bytes);

        let packet = PlayerActionPacket::decode(&mut reader, ProtocolVersion::V1_21).unwrap();

        assert_eq!(packet.status(), 0);
        assert!(!packet.is_drop_selected_item());
    }
}
