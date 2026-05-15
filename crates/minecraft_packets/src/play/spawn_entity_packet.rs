use crate::play::move_entity_packet::encode_angle;
use minecraft_protocol::prelude::*;

pub struct SpawnEntityPacket {
    entity_id: VarInt,
    entity_uuid: UuidAsLongs,
    x: f64,
    y: f64,
    z: f64,
    pitch: u8,
    yaw: u8,
    head_yaw: u8,
}

impl SpawnEntityPacket {
    pub fn spawn_player(
        entity_id: i32,
        uuid: Uuid,
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
    ) -> Self {
        Self {
            entity_id: VarInt::new(entity_id),
            entity_uuid: UuidAsLongs::new(uuid),
            x,
            y,
            z,
            pitch: encode_angle(pitch),
            yaw: encode_angle(yaw),
            head_yaw: encode_angle(yaw),
        }
    }
}

fn player_entity_type_id(version: ProtocolVersion) -> i32 {
    if version.is_after_inclusive(ProtocolVersion::V1_21_11) {
        155
    } else if version.is_after_inclusive(ProtocolVersion::V1_21_9) {
        151
    } else if version.is_after_inclusive(ProtocolVersion::V1_21_6) {
        149
    } else if version.is_after_inclusive(ProtocolVersion::V1_21_2) {
        148
    } else if version.is_after_inclusive(ProtocolVersion::V1_20_5) {
        128
    } else if version.is_after_inclusive(ProtocolVersion::V1_20_3) {
        124
    } else {
        122
    }
}

impl EncodePacket for SpawnEntityPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        self.entity_id.encode(writer, version)?;
        self.entity_uuid.encode(writer, version)?;
        VarInt::new(player_entity_type_id(version)).encode(writer, version)?;
        self.x.encode(writer, version)?;
        self.y.encode(writer, version)?;
        self.z.encode(writer, version)?;
        if version.is_after_inclusive(ProtocolVersion::V1_21_9) {
            encode_zero_lp_velocity(writer, version)?;
        }
        self.pitch.encode(writer, version)?;
        self.yaw.encode(writer, version)?;
        if version.is_after_inclusive(ProtocolVersion::V1_20_2) {
            self.head_yaw.encode(writer, version)?;
        }
        VarInt::new(0).encode(writer, version)?;
        if version.is_before_inclusive(ProtocolVersion::V1_21_7) {
            encode_zero_legacy_velocity(writer, version)?;
        }
        Ok(())
    }
}

fn encode_zero_lp_velocity(
    writer: &mut BinaryWriter,
    version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    0_u8.encode(writer, version)
}

fn encode_zero_legacy_velocity(
    writer: &mut BinaryWriter,
    version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    0_i16.encode(writer, version)?;
    0_i16.encode(writer, version)?;
    0_i16.encode(writer, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_entity_type_id_tracks_1_21_registry_shifts() {
        assert_eq!(player_entity_type_id(ProtocolVersion::V1_21), 128);
        assert_eq!(player_entity_type_id(ProtocolVersion::V1_21_4), 148);
        assert_eq!(player_entity_type_id(ProtocolVersion::V1_21_5), 148);
        assert_eq!(player_entity_type_id(ProtocolVersion::V1_21_6), 149);
        assert_eq!(player_entity_type_id(ProtocolVersion::V1_21_7), 149);
        assert_eq!(player_entity_type_id(ProtocolVersion::V1_21_9), 151);
        assert_eq!(player_entity_type_id(ProtocolVersion::V1_21_11), 155);
    }
}
