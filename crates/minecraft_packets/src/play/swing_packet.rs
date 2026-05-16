use minecraft_protocol::prelude::*;

pub struct SwingPacket {
    legacy_entity_id: Option<i32>,
    legacy_animation: Option<i8>,
    hand: Option<i32>,
}

impl SwingPacket {
    const LEGACY_MAIN_HAND_ANIMATION: i8 = 1;
    const MAIN_HAND: i32 = 0;

    pub fn triggers_main_hand_swing(
        &self,
        client_entity_id: i32,
        version: ProtocolVersion,
    ) -> bool {
        if version.is_before_inclusive(ProtocolVersion::V1_7_6) {
            return self.legacy_entity_id == Some(client_entity_id)
                && self.legacy_animation == Some(Self::LEGACY_MAIN_HAND_ANIMATION);
        }
        if version == ProtocolVersion::V1_8 {
            return true;
        }
        self.hand == Some(Self::MAIN_HAND)
    }
}

impl DecodePacket for SwingPacket {
    fn decode(
        reader: &mut BinaryReader,
        version: ProtocolVersion,
    ) -> Result<Self, BinaryReaderError> {
        if version.is_before_inclusive(ProtocolVersion::V1_7_6) {
            return Ok(Self {
                legacy_entity_id: Some(i32::decode(reader, version)?),
                legacy_animation: Some(i8::decode(reader, version)?),
                hand: None,
            });
        }

        if version == ProtocolVersion::V1_8 {
            return Ok(Self {
                legacy_entity_id: None,
                legacy_animation: None,
                hand: None,
            });
        }

        Ok(Self {
            legacy_entity_id: None,
            legacy_animation: None,
            hand: Some(VarInt::decode(reader, version)?.inner()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_v1_7_2_legacy_arm_animation() {
        let mut reader = BinaryReader::new(&[0, 0, 1, 44, 1]);
        let packet = SwingPacket::decode(&mut reader, ProtocolVersion::V1_7_2).unwrap();

        assert!(packet.triggers_main_hand_swing(300, ProtocolVersion::V1_7_2));
        assert!(!packet.triggers_main_hand_swing(301, ProtocolVersion::V1_7_2));
    }

    #[test]
    fn v1_8_empty_payload_is_main_hand() {
        let mut reader = BinaryReader::new(&[]);
        let packet = SwingPacket::decode(&mut reader, ProtocolVersion::V1_8).unwrap();

        assert!(packet.triggers_main_hand_swing(300, ProtocolVersion::V1_8));
    }

    #[test]
    fn decodes_modern_main_hand() {
        let mut reader = BinaryReader::new(&[0]);
        let packet = SwingPacket::decode(&mut reader, ProtocolVersion::V1_9).unwrap();

        assert!(packet.triggers_main_hand_swing(300, ProtocolVersion::V1_9));
    }

    #[test]
    fn decodes_modern_off_hand_without_triggering() {
        let mut reader = BinaryReader::new(&[1]);
        let packet = SwingPacket::decode(&mut reader, ProtocolVersion::V1_9).unwrap();

        assert!(!packet.triggers_main_hand_swing(300, ProtocolVersion::V1_9));
    }
}
