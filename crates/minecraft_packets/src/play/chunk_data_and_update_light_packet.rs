use crate::play::data::chunk_context::{VoidChunkContext, WorldContext};
use crate::play::data::chunk_data::ChunkData;
use crate::play::data::light_data::{LightData, LightDataLegacy};
use minecraft_protocol::prelude::*;

#[derive(PacketOut)]
pub struct ChunkDataAndUpdateLightPacket {
    chunk_x: i32,
    chunk_z: i32,

    #[pvn(..755)]
    full_chunk: bool,

    /// Added in V1_16 alongside the combined chunk+light packet; not present in pre-1.16 formats.
    #[pvn(735..751)]
    ignore_old_data: bool,

    /// BitSet primary bit mask (V1_17 only: 755–756).
    #[pvn(755..757)]
    v1_17_primary_bit_mask: LengthPaddedVec<u64>,

    /// VarInt primary bit mask (pre-V1_17).
    #[pvn(..755)]
    primary_bit_mask: VarInt,

    chunk_data: ChunkData,

    /// Present from V1_16 (when light was folded into this packet) through V1_19.4.
    #[pvn(735..763)]
    trust_edges: bool,

    /// V1_16–V1_16_4 (735–754): light masks are a single VarInt.
    #[pvn(735..755)]
    v1_16_light_data: LightDataLegacy,

    /// V1_17+ (755+): light masks are BitSet (array of i64).
    #[pvn(755..)]
    v1_18_light_data: LightData,
}

impl ChunkDataAndUpdateLightPacket {
    pub fn void(context: VoidChunkContext) -> Self {
        let dimension_height = context.dimension_height;
        Self {
            chunk_x: context.chunk_x,
            chunk_z: context.chunk_z,
            v1_17_primary_bit_mask: LengthPaddedVec::default(),
            primary_bit_mask: VarInt::default(),
            full_chunk: true,
            ignore_old_data: false,
            chunk_data: ChunkData::void(context),
            trust_edges: true,
            v1_16_light_data: LightDataLegacy::new_void(),
            v1_18_light_data: LightData::new_void(dimension_height),
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

        let (v1_18_light_data, v1_16_light_data) = match (
            schematic_context
                .world
                .get_chunk_sky_light(chunk_x, chunk_z),
            schematic_context
                .world
                .get_chunk_block_light(chunk_x, chunk_z),
        ) {
            (Some(sky_light), Some(block_light)) => (
                LightData::from_light_data(sky_light, block_light, chunk_context.dimension_height),
                LightDataLegacy::from_light_data(sky_light, block_light),
            ),
            _ => (
                LightData::new_void(chunk_context.dimension_height),
                LightDataLegacy::new_void(),
            ),
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
            v1_16_light_data,
            v1_18_light_data,
        }
    }
}
