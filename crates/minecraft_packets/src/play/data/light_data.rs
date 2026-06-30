use minecraft_protocol::prelude::*;
use pico_structures::prelude::LightSection;
use std::sync::{Arc, OnceLock};

#[derive(PacketOut, Default)]
pub struct LightData {
    sky_light_mask: BitSet,
    block_light_mask: BitSet,
    empty_sky_light_mask: BitSet,
    empty_block_light_mask: BitSet,
    sky_light_arrays: LengthPaddedVec<Light>,
    block_light_arrays: LengthPaddedVec<Light>,
}

#[derive(Default, Clone)]
pub struct Light {
    /// Length of the following array is always 2048
    /// There is 1 array for each bit set to true in the light mask, starting with the lowest value. Half a byte per light value. Indexed ((y<<8) | (z<<4) | x) / 2 If there's a remainder, masked 0xF0 else 0x0F.
    block_light_array: Arc<[u8]>,
}

impl Light {
    pub fn new(data: Vec<i8>) -> Self {
        Self {
            block_light_array: data.into_iter().map(|byte| byte as u8).collect(),
        }
    }

    pub fn full_sky() -> Self {
        static FULL_SKY_LIGHT: OnceLock<Arc<[u8]>> = OnceLock::new();

        Self {
            block_light_array: Arc::clone(FULL_SKY_LIGHT.get_or_init(|| vec![0xFF; 2048].into())),
        }
    }

    pub fn no_block() -> Self {
        static NO_BLOCK_LIGHT: OnceLock<Arc<[u8]>> = OnceLock::new();

        Self {
            block_light_array: Arc::clone(NO_BLOCK_LIGHT.get_or_init(|| vec![0; 2048].into())),
        }
    }

    /// Builds a per-section light array list: an `edge` array for the section
    /// below the chunk, one array per `sections` entry, then `edge` arrays
    /// padding out to `total_light_sections` (covering the section above the
    /// chunk and any sections the world doesn't have light data for).
    pub(crate) fn padded_arrays(
        sections: &[LightSection],
        total_light_sections: usize,
        edge: fn() -> Light,
    ) -> Vec<Light> {
        let mut arrays = Vec::with_capacity(total_light_sections);
        arrays.push(edge());
        arrays.extend(sections.iter().map(|section| Light::new(section.clone())));
        arrays.push(edge());
        while arrays.len() < total_light_sections {
            arrays.push(edge());
        }
        arrays
    }
}

impl EncodePacket for Light {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        VarInt::new(i32::try_from(self.block_light_array.len())?)
            .encode(writer, protocol_version)?;
        writer.write_bytes(&self.block_light_array)?;
        Ok(())
    }
}

impl LightData {
    pub fn from_light_data(
        sky_light_sections: &[LightSection],
        block_light_sections: &[LightSection],
        dimension_height: i32,
    ) -> Self {
        let world_section_count = dimension_height / 16;
        let total_light_sections = (world_section_count + 2) as u32;

        let all_sections_mask_val = (1u64 << total_light_sections) - 1;
        let all_sections_mask = BitSet::new(vec![all_sections_mask_val as i64]);

        let sky_light_arrays = Light::padded_arrays(
            sky_light_sections,
            total_light_sections as usize,
            Light::full_sky,
        );
        let block_light_arrays = Light::padded_arrays(
            block_light_sections,
            total_light_sections as usize,
            Light::no_block,
        );

        Self {
            sky_light_mask: all_sections_mask.clone(),
            block_light_mask: all_sections_mask.clone(),
            empty_sky_light_mask: BitSet::default(),
            empty_block_light_mask: BitSet::default(),
            sky_light_arrays: LengthPaddedVec::new(sky_light_arrays),
            block_light_arrays: LengthPaddedVec::new(block_light_arrays),
        }
    }

    pub fn new_void(dimension_height: i32) -> Self {
        let world_section_count = dimension_height / 16;
        let total_light_sections = (world_section_count + 2) as u32;

        let all_sections_mask_val = (1u64 << total_light_sections) - 1;
        let all_sections_mask = BitSet::new(vec![all_sections_mask_val as i64]);

        Self {
            sky_light_mask: all_sections_mask.clone(),
            block_light_mask: all_sections_mask.clone(),
            empty_sky_light_mask: BitSet::default(),
            empty_block_light_mask: BitSet::default(),
            sky_light_arrays: LengthPaddedVec::new(vec![
                Light::full_sky();
                total_light_sections as usize
            ]),
            block_light_arrays: LengthPaddedVec::new(vec![
                Light::no_block();
                total_light_sections as usize
            ]),
        }
    }
}
