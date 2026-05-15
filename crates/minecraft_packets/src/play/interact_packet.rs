use minecraft_protocol::prelude::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InteractAction {
    Interact,
    Attack,
    InteractAt,
    Unknown(i32),
}

pub struct InteractPacket {
    target_entity_id: i32,
    action: InteractAction,
    hand: Option<i32>,
    sneaking: Option<bool>,
}

impl InteractPacket {
    pub const fn target_entity_id(&self) -> i32 {
        self.target_entity_id
    }

    pub const fn action(&self) -> InteractAction {
        self.action
    }

    pub fn triggers_npc_interaction(&self) -> bool {
        match self.action {
            InteractAction::Interact | InteractAction::InteractAt => self.hand.unwrap_or(0) == 0,
            InteractAction::Attack => true,
            InteractAction::Unknown(_) => false,
        }
    }

    #[allow(dead_code)]
    pub const fn sneaking(&self) -> Option<bool> {
        self.sneaking
    }
}

impl DecodePacket for InteractPacket {
    fn decode(
        reader: &mut BinaryReader,
        version: ProtocolVersion,
    ) -> Result<Self, BinaryReaderError> {
        let target_entity_id = if version.is_before_inclusive(ProtocolVersion::V1_7_6) {
            i32::decode(reader, version)?
        } else {
            VarInt::decode(reader, version)?.inner()
        };

        if version.is_after_inclusive(ProtocolVersion::V26_1) {
            let hand = Some(VarInt::decode(reader, version)?.inner());
            let sneaking = Some(bool::decode(reader, version)?);

            return Ok(Self {
                target_entity_id,
                action: InteractAction::Interact,
                hand,
                sneaking,
            });
        }

        let action_id = if version.is_before_inclusive(ProtocolVersion::V1_7_6) {
            i8::decode(reader, version)?.into()
        } else {
            VarInt::decode(reader, version)?.inner()
        };

        let action = match action_id {
            0 => InteractAction::Interact,
            1 => InteractAction::Attack,
            2 => InteractAction::InteractAt,
            other => InteractAction::Unknown(other),
        };

        if matches!(action, InteractAction::InteractAt) {
            let _target_x = f32::decode(reader, version)?;
            let _target_y = f32::decode(reader, version)?;
            let _target_z = f32::decode(reader, version)?;
        }

        let hand = if version.is_after_inclusive(ProtocolVersion::V1_9)
            && matches!(
                action,
                InteractAction::Interact | InteractAction::InteractAt
            ) {
            Some(VarInt::decode(reader, version)?.inner())
        } else {
            None
        };

        let sneaking = if version.is_after_inclusive(ProtocolVersion::V1_16) {
            Some(bool::decode(reader, version)?)
        } else {
            None
        };

        Ok(Self {
            target_entity_id,
            action,
            hand,
            sneaking,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_current_interact_main_hand() {
        let mut reader = BinaryReader::new(&[0xac, 0x02, 0, 0, 0]);
        let packet = InteractPacket::decode(&mut reader, ProtocolVersion::V1_21).unwrap();

        assert_eq!(packet.target_entity_id(), 300);
        assert_eq!(packet.action(), InteractAction::Interact);
        assert!(packet.triggers_npc_interaction());
        assert_eq!(packet.sneaking(), Some(false));
    }

    #[test]
    fn decodes_interact_at_with_target_vector() {
        let mut data = vec![0xac, 0x02, 2];
        data.extend_from_slice(&1.0_f32.to_be_bytes());
        data.extend_from_slice(&2.0_f32.to_be_bytes());
        data.extend_from_slice(&3.0_f32.to_be_bytes());
        data.extend_from_slice(&[0, 1]);

        let mut reader = BinaryReader::new(&data);
        let packet = InteractPacket::decode(&mut reader, ProtocolVersion::V1_21).unwrap();

        assert_eq!(packet.action(), InteractAction::InteractAt);
        assert!(packet.triggers_npc_interaction());
        assert_eq!(packet.sneaking(), Some(true));
    }

    #[test]
    fn offhand_interaction_is_not_primary() {
        let mut reader = BinaryReader::new(&[0xac, 0x02, 0, 1, 0]);
        let packet = InteractPacket::decode(&mut reader, ProtocolVersion::V1_21).unwrap();

        assert!(!packet.triggers_npc_interaction());
    }

    #[test]
    fn attack_triggers_npc_interaction_for_legacy_interact_packets() {
        let mut reader = BinaryReader::new(&[0xac, 0x02, 1, 0]);
        let packet = InteractPacket::decode(&mut reader, ProtocolVersion::V1_21).unwrap();

        assert_eq!(packet.action(), InteractAction::Attack);
        assert!(packet.triggers_npc_interaction());
    }

    #[test]
    fn decodes_current_right_click_without_legacy_action() {
        let mut reader = BinaryReader::new(&[0xac, 0x02, 0, 0]);
        let packet = InteractPacket::decode(&mut reader, ProtocolVersion::V26_1).unwrap();

        assert_eq!(packet.target_entity_id(), 300);
        assert_eq!(packet.action(), InteractAction::Interact);
        assert!(packet.triggers_npc_interaction());
        assert_eq!(packet.sneaking(), Some(false));
    }
}
