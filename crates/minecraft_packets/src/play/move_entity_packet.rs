use minecraft_protocol::prelude::*;

const POSITION_DELTA_SCALE: f64 = 4096.0;
const LEGACY_POSITION_DELTA_SCALE: f64 = 32.0;
const LEGACY_DELTA_CONVERSION_SCALE: f64 = POSITION_DELTA_SCALE / LEGACY_POSITION_DELTA_SCALE;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RelativeMoveDeltaError {
    OutOfRange,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RelativeMoveDelta {
    x: i16,
    y: i16,
    z: i16,
}

impl RelativeMoveDelta {
    pub fn between(
        previous: (f64, f64, f64),
        current: (f64, f64, f64),
    ) -> Result<Self, RelativeMoveDeltaError> {
        Self::between_for_version(previous, current, ProtocolVersion::V1_9)
    }

    pub fn between_for_version(
        previous: (f64, f64, f64),
        current: (f64, f64, f64),
        protocol_version: ProtocolVersion,
    ) -> Result<Self, RelativeMoveDeltaError> {
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_8) {
            legacy_scaled_delta(current.0 - previous.0)?;
            legacy_scaled_delta(current.1 - previous.1)?;
            legacy_scaled_delta(current.2 - previous.2)?;
        }
        Ok(Self {
            x: scaled_delta(current.0 - previous.0)?,
            y: scaled_delta(current.1 - previous.1)?,
            z: scaled_delta(current.2 - previous.2)?,
        })
    }

    pub const fn new_unchecked(x: i16, y: i16, z: i16) -> Self {
        Self { x, y, z }
    }
}

fn scaled_delta(delta: f64) -> Result<i16, RelativeMoveDeltaError> {
    let scaled = (delta * POSITION_DELTA_SCALE).round();
    if scaled < f64::from(i16::MIN) || scaled > f64::from(i16::MAX) {
        return Err(RelativeMoveDeltaError::OutOfRange);
    }
    Ok(scaled as i16)
}

fn legacy_scaled_delta(delta: f64) -> Result<i8, RelativeMoveDeltaError> {
    let scaled = (delta * LEGACY_POSITION_DELTA_SCALE).round();
    if scaled < f64::from(i8::MIN) || scaled > f64::from(i8::MAX) {
        return Err(RelativeMoveDeltaError::OutOfRange);
    }
    Ok(scaled as i8)
}

fn legacy_scaled_delta_from_modern_units(delta: i16) -> i8 {
    (f64::from(delta) / LEGACY_DELTA_CONVERSION_SCALE).round() as i8
}

pub struct MoveEntityPosPacket {
    entity_id: VarInt,
    delta_x: i16,
    delta_y: i16,
    delta_z: i16,
    on_ground: bool,
}

impl MoveEntityPosPacket {
    pub fn new(entity_id: i32, delta: RelativeMoveDelta, on_ground: bool) -> Self {
        Self {
            entity_id: VarInt::new(entity_id),
            delta_x: delta.x,
            delta_y: delta.y,
            delta_z: delta.z,
            on_ground,
        }
    }
}

impl EncodePacket for MoveEntityPosPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        encode_entity_id(&self.entity_id, writer, protocol_version)?;
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_8) {
            legacy_scaled_delta_from_modern_units(self.delta_x).encode(writer, protocol_version)?;
            legacy_scaled_delta_from_modern_units(self.delta_y).encode(writer, protocol_version)?;
            legacy_scaled_delta_from_modern_units(self.delta_z).encode(writer, protocol_version)?;
        } else {
            self.delta_x.encode(writer, protocol_version)?;
            self.delta_y.encode(writer, protocol_version)?;
            self.delta_z.encode(writer, protocol_version)?;
        }
        if protocol_version.is_after_inclusive(ProtocolVersion::V1_8) {
            self.on_ground.encode(writer, protocol_version)?;
        }
        Ok(())
    }
}

pub struct MoveEntityPosRotPacket {
    entity_id: VarInt,
    delta_x: i16,
    delta_y: i16,
    delta_z: i16,
    yaw: u8,
    pitch: u8,
    on_ground: bool,
}

impl MoveEntityPosRotPacket {
    pub fn new(
        entity_id: i32,
        delta: RelativeMoveDelta,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    ) -> Self {
        Self {
            entity_id: VarInt::new(entity_id),
            delta_x: delta.x,
            delta_y: delta.y,
            delta_z: delta.z,
            yaw: encode_angle(yaw),
            pitch: encode_angle(pitch),
            on_ground,
        }
    }
}

impl EncodePacket for MoveEntityPosRotPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        encode_entity_id(&self.entity_id, writer, protocol_version)?;
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_8) {
            legacy_scaled_delta_from_modern_units(self.delta_x).encode(writer, protocol_version)?;
            legacy_scaled_delta_from_modern_units(self.delta_y).encode(writer, protocol_version)?;
            legacy_scaled_delta_from_modern_units(self.delta_z).encode(writer, protocol_version)?;
        } else {
            self.delta_x.encode(writer, protocol_version)?;
            self.delta_y.encode(writer, protocol_version)?;
            self.delta_z.encode(writer, protocol_version)?;
        }
        self.yaw.encode(writer, protocol_version)?;
        self.pitch.encode(writer, protocol_version)?;
        if protocol_version.is_after_inclusive(ProtocolVersion::V1_8) {
            self.on_ground.encode(writer, protocol_version)?;
        }
        Ok(())
    }
}

pub struct MoveEntityRotPacket {
    entity_id: VarInt,
    yaw: u8,
    pitch: u8,
    on_ground: bool,
}

impl MoveEntityRotPacket {
    pub fn new(entity_id: i32, yaw: f32, pitch: f32, on_ground: bool) -> Self {
        Self {
            entity_id: VarInt::new(entity_id),
            yaw: encode_angle(yaw),
            pitch: encode_angle(pitch),
            on_ground,
        }
    }
}

impl EncodePacket for MoveEntityRotPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        encode_entity_id(&self.entity_id, writer, protocol_version)?;
        self.yaw.encode(writer, protocol_version)?;
        self.pitch.encode(writer, protocol_version)?;
        if protocol_version.is_after_inclusive(ProtocolVersion::V1_8) {
            self.on_ground.encode(writer, protocol_version)?;
        }
        Ok(())
    }
}

pub fn encode_entity_id(
    entity_id: &VarInt,
    writer: &mut BinaryWriter,
    protocol_version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    if protocol_version.is_before_inclusive(ProtocolVersion::V1_7_6) {
        entity_id.inner().encode(writer, protocol_version)
    } else {
        entity_id.encode(writer, protocol_version)
    }
}

pub fn encode_angle(angle: f32) -> u8 {
    ((angle.rem_euclid(360.0) * 256.0 / 360.0).floor() as u16 & 0xff) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_common_angles() {
        assert_eq!(encode_angle(0.0), 0);
        assert_eq!(encode_angle(90.0), 64);
        assert_eq!(encode_angle(180.0), 128);
        assert_eq!(encode_angle(270.0), 192);
        assert_eq!(encode_angle(-90.0), 192);
    }

    #[test]
    fn relative_move_delta_converts_and_validates_both_eras() {
        // Modern (1.9+): scaled by 4096, stored as i16.
        assert_eq!(
            RelativeMoveDelta::between((1.0, 2.0, 3.0), (1.5, 1.75, 3.125)).unwrap(),
            RelativeMoveDelta::new_unchecked(2048, -1024, 512)
        );
        assert_eq!(
            RelativeMoveDelta::between((0.0, 0.0, 0.0), (8.1, 0.0, 0.0)),
            Err(RelativeMoveDeltaError::OutOfRange)
        );

        // Legacy (≤1.8): also scaled by 4096 internally, but validated against i8 range.
        assert_eq!(
            RelativeMoveDelta::between_for_version(
                (1.0, 2.0, 3.0),
                (1.5, 1.75, 3.125),
                ProtocolVersion::V1_8
            )
            .unwrap(),
            RelativeMoveDelta::new_unchecked(2048, -1024, 512)
        );
        assert_eq!(
            RelativeMoveDelta::between_for_version(
                (0.0, 0.0, 0.0),
                (4.0, 0.0, 0.0),
                ProtocolVersion::V1_8
            ),
            Err(RelativeMoveDeltaError::OutOfRange)
        );
    }
}
