use crate::play::move_entity_packet::{encode_angle, encode_entity_id};
use minecraft_protocol::prelude::*;

const LEGACY_ABSOLUTE_POSITION_SCALE: f64 = 32.0;

pub struct TeleportEntityPacket {
    entity_id: VarInt,
    x: f64,
    y: f64,
    z: f64,
    yaw: u8,
    pitch: u8,
    on_ground: bool,
}

impl EncodePacket for TeleportEntityPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if version.is_after_inclusive(ProtocolVersion::V1_21_2) {
            return Err(BinaryWriterError::UnsupportedOperation);
        }
        encode_entity_id(&self.entity_id, writer, version)?;
        if version.is_before_inclusive(ProtocolVersion::V1_8) {
            legacy_absolute_position(self.x).encode(writer, version)?;
            legacy_absolute_position(self.y).encode(writer, version)?;
            legacy_absolute_position(self.z).encode(writer, version)?;
        } else {
            self.x.encode(writer, version)?;
            self.y.encode(writer, version)?;
            self.z.encode(writer, version)?;
        }
        self.yaw.encode(writer, version)?;
        self.pitch.encode(writer, version)?;
        if version.is_after_inclusive(ProtocolVersion::V1_8) {
            self.on_ground.encode(writer, version)?;
        }
        Ok(())
    }
}

impl TeleportEntityPacket {
    pub fn absolute(
        entity_id: i32,
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    ) -> Self {
        Self {
            entity_id: VarInt::new(entity_id),
            x,
            y,
            z,
            yaw: encode_angle(yaw),
            pitch: encode_angle(pitch),
            on_ground,
        }
    }
}

fn legacy_absolute_position(position: f64) -> i32 {
    (position * LEGACY_ABSOLUTE_POSITION_SCALE).floor() as i32
}

pub struct EntityPositionSyncPacket {
    entity_id: VarInt,
    x: f64,
    y: f64,
    z: f64,
    velocity_x: f64,
    velocity_y: f64,
    velocity_z: f64,
    yaw: f32,
    pitch: f32,
    on_ground: bool,
}

impl EntityPositionSyncPacket {
    pub fn absolute(
        entity_id: i32,
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    ) -> Self {
        Self {
            entity_id: VarInt::new(entity_id),
            x,
            y,
            z,
            velocity_x: 0.0,
            velocity_y: 0.0,
            velocity_z: 0.0,
            yaw,
            pitch,
            on_ground,
        }
    }
}

impl EncodePacket for EntityPositionSyncPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        self.entity_id.encode(writer, version)?;
        if version.is_after_inclusive(ProtocolVersion::V26_3) {
            VarInt::new(1).encode(writer, version)?; // Stepped position
            VarInt::new(1).encode(writer, version)?; // One step
        }
        self.x.encode(writer, version)?;
        self.y.encode(writer, version)?;
        self.z.encode(writer, version)?;
        if version.is_after_inclusive(ProtocolVersion::V26_3) {
            VarInt::new(3).encode(writer, version)?; // Interpolate over three ticks
        } else {
            self.velocity_x.encode(writer, version)?;
            self.velocity_y.encode(writer, version)?;
            self.velocity_z.encode(writer, version)?;
        }
        self.yaw.encode(writer, version)?;
        self.pitch.encode(writer, version)?;
        self.on_ground.encode(writer, version)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn position_sync_26_3_uses_stepped_positions() {
        let packet = EntityPositionSyncPacket::absolute(1, 0.0, 0.0, 0.0, 0.0, 0.0, true);
        let mut writer = BinaryWriter::default();
        packet.encode(&mut writer, ProtocolVersion::V26_3).unwrap();
        let mut expected = vec![1, 1, 1]; // Entity, stepped mode, step count
        expected.extend([0; 24]); // XYZ
        expected.push(3); // Step duration
        expected.extend([0; 8]); // Rotation
        expected.push(1); // On ground
        assert_eq!(writer.as_slice(), expected);
        let mut legacy = BinaryWriter::default();
        packet.encode(&mut legacy, ProtocolVersion::V26_2).unwrap();
        assert_eq!(legacy.as_slice().len(), 58);
    }
}
