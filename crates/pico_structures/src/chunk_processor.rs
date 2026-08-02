use crate::pack_direct::pack_direct;
use crate::palette::Palette;
use crate::prelude::Schematic;
use blocks_report::InternalId;
use minecraft_protocol::prelude::Coordinates;
use std::mem;
use thiserror::Error;

// Palette indices are capped at 256 entries, so a u16 is enough for both a
// valid index and the sentinel. This halves the reusable lookup table without
// changing the packed output type.
const UNSEEN_ID_INDEX: u16 = u16::MAX;

const LOOKUP_TABLE_SIZE: usize = InternalId::MAX as usize + 1;

/// A helper struct to hold reusable buffers for palette generation.
/// This avoids re-allocating the HashMap and palette Vec for every chunk.
/// The main block data is now handled on the stack.
pub struct ChunkProcessor {
    palette: Vec<InternalId>,
    id_to_palette_index: Vec<u16>,
}

#[derive(Debug, Error)]
pub enum ChunkProcessorError {
    #[error("The palette must not be empty")]
    EmptyPalette,
}

impl ChunkProcessor {
    const MAX_PALETTED_SIZE: usize = 256;

    pub fn new() -> Self {
        Self {
            palette: Vec::with_capacity(Self::MAX_PALETTED_SIZE),
            id_to_palette_index: vec![UNSEEN_ID_INDEX; LOOKUP_TABLE_SIZE],
        }
    }

    fn prepare_for_next_chunk(&mut self) {
        for &id in &self.palette {
            self.id_to_palette_index[id as usize] = UNSEEN_ID_INDEX;
        }
        self.palette.clear();
    }

    fn take_palette(&mut self) -> Vec<InternalId> {
        let palette = mem::take(&mut self.palette);
        for &id in &palette {
            self.id_to_palette_index[id as usize] = UNSEEN_ID_INDEX;
        }
        palette
    }

    pub fn process_section(
        &mut self,
        schematic: &Schematic,
        section_position: Coordinates,
    ) -> Result<Palette, ChunkProcessorError> {
        const SECTION_VOLUME: usize = 4096;
        const SECTION_SIZE: i32 = 16;

        self.prepare_for_next_chunk();

        let mut block_ids: [InternalId; SECTION_VOLUME] = [0; SECTION_VOLUME];

        let section_origin = section_position * SECTION_SIZE;
        let mut first_id: Option<InternalId> = None;
        let mut is_single_block = true;
        let mut block_index = 0;

        for y in 0..SECTION_SIZE {
            for z in 0..SECTION_SIZE {
                for x in 0..SECTION_SIZE {
                    let world_pos = section_origin + Coordinates::new(x, y, z);
                    let internal_id = schematic.get_block_state_id(world_pos).internal_id();

                    block_ids[block_index] = internal_id;
                    block_index += 1;

                    if let Some(id) = first_id {
                        if is_single_block && id != internal_id {
                            is_single_block = false;
                        }
                    } else {
                        first_id = Some(internal_id);
                    }

                    let palette_index_slot = &mut self.id_to_palette_index[internal_id as usize];

                    if *palette_index_slot == UNSEEN_ID_INDEX {
                        let new_index = self.palette.len() as u16;
                        self.palette.push(internal_id);
                        *palette_index_slot = new_index;
                    }
                }
            }
        }

        if is_single_block {
            return if let Some(id) = first_id {
                Ok(Palette::single(id))
            } else {
                Err(ChunkProcessorError::EmptyPalette)
            };
        }

        let bits_per_entry = bits_needed(self.palette.len() as u32);

        if bits_per_entry <= 8 {
            let bits_per_entry = bits_per_entry.clamp(4, 8) as u8;

            let paletted_data = block_ids
                .iter()
                .map(|&id| u32::from(self.id_to_palette_index[id as usize]));
            let packed_data = pack_direct(paletted_data, bits_per_entry);

            Ok(Palette::paletted(
                bits_per_entry,
                self.take_palette(),
                packed_data,
            ))
        } else {
            Ok(Palette::direct(block_ids.to_vec()))
        }
    }
}

/// Calculates the minimum number of bits required to represent `n` distinct states.
fn bits_needed(n: u32) -> u32 {
    if n <= 1 { 1 } else { (n - 1).ilog2() + 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::Schematic;

    #[test]
    fn reusing_processor_rebuilds_palette_after_paletted_section() {
        let dimensions = Coordinates::new(16, 32, 16);
        let mut block_data = vec![0; (16 * 32 * 16) as usize];
        block_data[1] = 1;
        block_data[16 * 16 * 16 + 1] = 1;
        let schematic = Schematic::from_test_data(dimensions, vec![0, 1], block_data);
        let mut processor = ChunkProcessor::new();

        let first = processor
            .process_section(&schematic, Coordinates::new(0, 0, 0))
            .expect("first section should be processable");
        let second = processor
            .process_section(&schematic, Coordinates::new(0, 1, 0))
            .expect("second section should be processable");

        for palette in [first, second] {
            let Palette::Paletted {
                internal_palette,
                packed_data: _,
                bits_per_entry: _,
            } = palette
            else {
                panic!("expected a paletted section");
            };
            assert_eq!(internal_palette, vec![0, 1]);
        }
    }
}
