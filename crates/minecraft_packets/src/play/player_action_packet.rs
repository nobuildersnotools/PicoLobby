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
        // 26.3 inserted Change Destroy Direction at index 1. Keep the lobby's
        // action codes stable, and avoid treating that new action as a drop.
        let status = if version.is_after_inclusive(ProtocolVersion::V26_3) {
            match status {
                1 => -1,
                2.. => status - 1,
                _ => status,
            }
        } else {
            status
        };
        Ok(Self { status })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_drop_actions_shift_in_26_3() {
        for (status, expected) in [(1, false), (3, false), (4, true), (5, true), (6, false)] {
            let bytes = [status];
            let packet =
                PlayerActionPacket::decode(&mut BinaryReader::new(&bytes), ProtocolVersion::V26_3)
                    .unwrap();
            assert_eq!(packet.is_drop_selected_item(), expected, "status {status}");
        }
    }

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
