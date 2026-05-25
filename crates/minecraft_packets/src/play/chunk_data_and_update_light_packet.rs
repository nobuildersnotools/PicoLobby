use crate::play::data::chunk_context::{VoidChunkContext, WorldContext};
use crate::play::data::chunk_data::ChunkData;
use crate::play::data::light_data::LightData;
use minecraft_protocol::prelude::*;

/// This packet is only mandatory for versions above 1.20.3,
/// thus the packet is only implemented to work on versions after 1.20.3.
/// The GameEventPacket must be sent before sending this one.
pub struct ChunkDataAndUpdateLightPacket {
    chunk_x: i32,
    chunk_z: i32,

    full_chunk: bool,

    /// If false, the client will recalculate lighting based on the old/new chunk data
    ignore_old_data: bool,

    /// BitSet with bits (world height in blocks / 16) set to 1 for every 16×16×16 chunk section whose data is included in Data. The least significant bit represents the chunk section at the bottom of the chunk column (from the lowest y to 15 blocks above).
    /// Up until 1.17.1 included
    v1_17_primary_bit_mask: LengthPaddedVec<u64>, // availableSections bitset?

    primary_bit_mask: VarInt,

    chunk_data: ChunkData,

    /// If edges should be trusted for light updates.
    /// Up until 1.19.4 included
    trust_edges: bool,

    // TODO: Implement Update Light packet for versions prior to 1.18
    v1_18_light_data: LightData,
}

impl EncodePacket for ChunkDataAndUpdateLightPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        self.chunk_x.encode(writer, protocol_version)?;
        self.chunk_z.encode(writer, protocol_version)?;

        if protocol_version.is_before_inclusive(ProtocolVersion::V1_15_2) {
            self.full_chunk.encode(writer, protocol_version)?;
            if protocol_version.is_before_inclusive(ProtocolVersion::V1_7_6) {
                (self.primary_bit_mask.inner() as u16).encode(writer, protocol_version)?;
                0u16.encode(writer, protocol_version)?;
            } else if protocol_version.is_before_inclusive(ProtocolVersion::V1_8) {
                (self.primary_bit_mask.inner() as u16).encode(writer, protocol_version)?;
            } else {
                self.primary_bit_mask.encode(writer, protocol_version)?;
            }
            self.chunk_data.encode(writer, protocol_version)?;
            return Ok(());
        }

        if protocol_version.is_before_inclusive(ProtocolVersion::V1_16_4) {
            self.full_chunk.encode(writer, protocol_version)?;
        }
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_16_1) {
            self.ignore_old_data.encode(writer, protocol_version)?;
        }
        if protocol_version.between_inclusive(ProtocolVersion::V1_17, ProtocolVersion::V1_17_1) {
            self.v1_17_primary_bit_mask
                .encode(writer, protocol_version)?;
        }
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_16_4) {
            self.primary_bit_mask.encode(writer, protocol_version)?;
        }

        self.chunk_data.encode(writer, protocol_version)?;

        if protocol_version.between_inclusive(ProtocolVersion::V1_18, ProtocolVersion::V1_19_4) {
            self.trust_edges.encode(writer, protocol_version)?;
        }
        if protocol_version.is_after_inclusive(ProtocolVersion::V1_18) {
            self.v1_18_light_data.encode(writer, protocol_version)?;
        }

        Ok(())
    }
}

impl ChunkDataAndUpdateLightPacket {
    pub fn void(context: VoidChunkContext) -> Self {
        Self::void_with_light_data(context, LightData::new_void(context.dimension_height))
    }

    pub fn void_for_protocol(context: VoidChunkContext, protocol_version: ProtocolVersion) -> Self {
        let light_data = if protocol_version.is_after_inclusive(ProtocolVersion::V1_18) {
            LightData::new_void(context.dimension_height)
        } else {
            LightData::default()
        };

        Self::void_with_light_data(context, light_data)
    }

    fn void_with_light_data(context: VoidChunkContext, light_data: LightData) -> Self {
        let dimension_height = context.dimension_height;
        let all_sections_bit_mask = (1i32 << (dimension_height / 16).min(16)) - 1;
        Self {
            chunk_x: context.chunk_x,
            chunk_z: context.chunk_z,
            v1_17_primary_bit_mask: LengthPaddedVec::new(vec![all_sections_bit_mask as u64]),
            primary_bit_mask: VarInt::new(all_sections_bit_mask),
            full_chunk: true,
            ignore_old_data: false,
            chunk_data: ChunkData::void(context),
            trust_edges: true,
            v1_18_light_data: light_data,
        }
    }

    pub fn from_structure(
        chunk_context: VoidChunkContext,
        schematic_context: &WorldContext,
        protocol_version: ProtocolVersion,
    ) -> Self {
        let all_sections_bit_mask = 0b1111_1111_1111_1111i32;
        let chunk_x = chunk_context.chunk_x;
        let chunk_z = chunk_context.chunk_z;

        let light_data = if protocol_version.is_after_inclusive(ProtocolVersion::V1_18) {
            match (
                schematic_context
                    .world
                    .get_chunk_sky_light(chunk_x, chunk_z),
                schematic_context
                    .world
                    .get_chunk_block_light(chunk_x, chunk_z),
            ) {
                (Some(sky_light), Some(block_light)) => LightData::from_light_data(
                    sky_light,
                    block_light,
                    chunk_context.dimension_height,
                ),
                _ => LightData::new_void(chunk_context.dimension_height),
            }
        } else {
            LightData::default()
        };

        Self {
            chunk_x,
            chunk_z,
            v1_17_primary_bit_mask: LengthPaddedVec::new(vec![all_sections_bit_mask as u64]),
            primary_bit_mask: VarInt::new(all_sections_bit_mask),
            full_chunk: true,
            ignore_old_data: false,
            chunk_data: ChunkData::from_schematic(
                chunk_context,
                schematic_context,
                protocol_version,
            ),
            trust_edges: true,
            v1_18_light_data: light_data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn void_packet() -> ChunkDataAndUpdateLightPacket {
        ChunkDataAndUpdateLightPacket::void(VoidChunkContext {
            chunk_x: 0,
            chunk_z: 0,
            biome_index: 1,
            dimension_height: 256,
            dimension_min_y: 0,
        })
    }

    #[test]
    fn v1_17_chunk_starts_with_section_bitset_then_heightmap() {
        for protocol_version in [ProtocolVersion::V1_17, ProtocolVersion::V1_17_1] {
            let packet = void_packet();
            let mut writer = BinaryWriter::default();

            packet.encode(&mut writer, protocol_version).unwrap();

            let bytes = writer.into_inner();
            assert_eq!(
                0x01, bytes[8],
                "1.17 chunk section bitset must start immediately after chunk coordinates"
            );
            assert_eq!(
                &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF],
                &bytes[9..17],
                "1.17 section bitset should include all 16 chunk sections"
            );
            assert_eq!(
                0x0A, bytes[17],
                "heightmap NBT root must follow the 1.17 section bitset"
            );
        }
    }

    #[test]
    fn v1_16_4_chunk_keeps_full_chunk_flag_before_primary_mask() {
        let packet = void_packet();
        let mut writer = BinaryWriter::default();

        packet
            .encode(&mut writer, ProtocolVersion::V1_16_4)
            .unwrap();

        let bytes = writer.into_inner();
        assert_eq!(0x01, bytes[8], "1.16.4 still includes full_chunk");
        assert_eq!(
            &[0xFF, 0xFF, 0x03],
            &bytes[9..12],
            "1.16.4 primary mask should follow full_chunk"
        );
        assert_eq!(0x0A, bytes[12], "heightmap NBT root should follow the mask");
    }
}
