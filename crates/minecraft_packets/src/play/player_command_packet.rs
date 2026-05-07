use minecraft_protocol::prelude::*;

#[derive(PacketIn)]
pub struct PlayerCommandPacket {
    entity_id: VarInt,
    action_id: VarInt,
    jump_boost: VarInt,
}

impl PlayerCommandPacket {
    const START_SNEAKING: i32 = 0;
    const STOP_SNEAKING: i32 = 1;

    pub fn entity_id(&self) -> i32 {
        self.entity_id.inner()
    }

    pub fn crouching_change(&self) -> Option<bool> {
        match self.action_id.inner() {
            Self::START_SNEAKING => Some(true),
            Self::STOP_SNEAKING => Some(false),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn jump_boost(&self) -> i32 {
        self.jump_boost.inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(data: &[u8]) -> PlayerCommandPacket {
        let mut reader = BinaryReader::new(data);
        PlayerCommandPacket::decode(&mut reader, ProtocolVersion::V1_21).unwrap()
    }

    #[test]
    fn decodes_start_and_stop_sneaking_actions() {
        let packet = decode(&[0xac, 0x02, 0, 0]);
        assert_eq!(packet.entity_id(), 300);
        assert_eq!(packet.crouching_change(), Some(true));

        let packet = decode(&[0xac, 0x02, 1, 0]);
        assert_eq!(packet.entity_id(), 300);
        assert_eq!(packet.crouching_change(), Some(false));
    }

    #[test]
    fn ignores_non_crouching_actions() {
        let packet = decode(&[0xac, 0x02, 3, 4]);

        assert_eq!(packet.crouching_change(), None);
        assert_eq!(packet.jump_boost(), 4);
    }
}
