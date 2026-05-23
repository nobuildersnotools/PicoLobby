use crate::server::packet_registry::PacketRegistry;
use blocks_report::get_block_report_id_mapping;
use minecraft_packets::play::chunk_data_and_update_light_packet::ChunkDataAndUpdateLightPacket;
use minecraft_packets::play::light_update_packet::LightUpdatePacket;
use minecraft_packets::play::{VoidChunkContext, WorldContext};
use minecraft_protocol::prelude::{Coordinates, ProtocolVersion};
use pico_registries::registry_provider::DimensionInfo;
use pico_structures::prelude::World;
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Copy, Clone)]
enum Direction {
    Right,
    Up,
    Left,
    Down,
}

impl Direction {
    const fn turn(self) -> Self {
        match self {
            Self::Right => Self::Up,
            Self::Up => Self::Left,
            Self::Left => Self::Down,
            Self::Down => Self::Right,
        }
    }

    const fn step(self, x: &mut i32, y: &mut i32) {
        match self {
            Self::Right => *x += 1,
            Self::Up => *y -= 1,
            Self::Left => *x -= 1,
            Self::Down => *y += 1,
        }
    }
}

struct SpiralIterator {
    center_x: i32,
    center_y: i32,
    current_x: i32,
    current_y: i32,
    direction: Direction,
    leg_length: i32,
    steps_remaining_in_leg: i32,
    grow_next_leg: bool,
    max_radius: i32,
}

impl SpiralIterator {
    const fn new(center_x: i32, center_y: i32, max_radius: i32) -> Self {
        Self {
            center_x,
            center_y,
            current_x: center_x,
            current_y: center_y,
            direction: Direction::Right,
            leg_length: 1,
            steps_remaining_in_leg: 1,
            grow_next_leg: false,
            max_radius,
        }
    }
}

impl Iterator for SpiralIterator {
    type Item = (i32, i32);

    fn next(&mut self) -> Option<Self::Item> {
        // Stop when the next position to yield is outside the allowed radius.
        let distance_x = (self.current_x - self.center_x).abs();
        let distance_y = (self.current_y - self.center_y).abs();
        if distance_x.max(distance_y) > self.max_radius {
            return None;
        }

        // Yield current position.
        let result = (self.current_x, self.current_y);

        // Advance state for the next call.
        self.direction
            .step(&mut self.current_x, &mut self.current_y);
        self.steps_remaining_in_leg -= 1;

        if self.steps_remaining_in_leg == 0 {
            self.direction = self.direction.turn();

            if self.grow_next_leg {
                self.leg_length += 1;
            }
            self.grow_next_leg = !self.grow_next_leg;

            self.steps_remaining_in_leg = self.leg_length;
        }

        Some(result)
    }
}

pub struct CircularChunkPacketIterator {
    biome_index: i32,
    pub dimension_height: i32,
    pub dimension_min_y: i32,
    schematic_context: Option<WorldContext>,
    spiral_iterator: SpiralIterator,
    protocol_version: ProtocolVersion,
    pending_packets: VecDeque<PacketRegistry>,
}

impl CircularChunkPacketIterator {
    pub fn new(
        center_chunk: (i32, i32),
        view_distance: i32,
        world: Option<Arc<World>>,
        biome_index: i32,
        dimension_info: &DimensionInfo,
        protocol_version: ProtocolVersion,
    ) -> Self {
        let (center_x, center_z) = center_chunk;
        let paste_origin = Coordinates::new_uniform(0);

        let mapping_version = if protocol_version.is_before_inclusive(ProtocolVersion::V1_15_2) {
            ProtocolVersion::V1_16
        } else {
            protocol_version
        };
        let schematic_context: Option<WorldContext> = get_block_report_id_mapping(mapping_version)
            .map_or(None, |report_id_mapping| {
                world.map(|world_arc| WorldContext {
                    paste_origin,
                    world: world_arc,
                    report_id_mapping,
                })
            });

        Self {
            biome_index,
            dimension_height: dimension_info.height,
            dimension_min_y: dimension_info.min_y,
            schematic_context,
            spiral_iterator: SpiralIterator::new(center_x, center_z, view_distance),
            protocol_version,
            pending_packets: VecDeque::new(),
        }
    }

    fn legacy_light_update_packet(
        &self,
        chunk_context: VoidChunkContext,
    ) -> Option<PacketRegistry> {
        if !self
            .protocol_version
            .between_inclusive(ProtocolVersion::V1_14, ProtocolVersion::V1_17_1)
        {
            return None;
        }

        let packet = self.schematic_context.as_ref().map_or_else(
            || {
                LightUpdatePacket::new_void(
                    chunk_context.chunk_x,
                    chunk_context.chunk_z,
                    chunk_context.dimension_height,
                )
            },
            |context| match (
                context
                    .world
                    .get_chunk_sky_light(chunk_context.chunk_x, chunk_context.chunk_z),
                context
                    .world
                    .get_chunk_block_light(chunk_context.chunk_x, chunk_context.chunk_z),
            ) {
                (Some(sky_light), Some(block_light)) => LightUpdatePacket::from_light_data(
                    chunk_context.chunk_x,
                    chunk_context.chunk_z,
                    sky_light,
                    block_light,
                    chunk_context.dimension_height,
                ),
                _ => LightUpdatePacket::new_void(
                    chunk_context.chunk_x,
                    chunk_context.chunk_z,
                    chunk_context.dimension_height,
                ),
            },
        );

        Some(PacketRegistry::LightUpdate(Box::new(packet)))
    }
}

impl Iterator for CircularChunkPacketIterator {
    type Item = PacketRegistry;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(packet) = self.pending_packets.pop_front() {
            return Some(packet);
        }

        let (chunk_x, chunk_z) = self.spiral_iterator.next()?;

        let chunk_context = VoidChunkContext {
            chunk_x,
            chunk_z,
            biome_index: self.biome_index,
            dimension_height: self.dimension_height,
            dimension_min_y: self.dimension_min_y,
        };

        let packet = match &self.schematic_context {
            Some(context) => ChunkDataAndUpdateLightPacket::from_structure(
                chunk_context,
                context,
                self.protocol_version,
            ),
            None => ChunkDataAndUpdateLightPacket::void(chunk_context),
        };

        if let Some(light_update_packet) = self.legacy_light_update_packet(chunk_context) {
            self.pending_packets.push_back(light_update_packet);
        }

        Some(PacketRegistry::ChunkDataAndUpdateLight(Box::new(packet)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_registries::Identifier;

    fn dimension_info() -> DimensionInfo {
        DimensionInfo {
            height: 256,
            min_y: 0,
            protocol_id: 0,
            registry_key: Identifier::vanilla_unchecked("overworld"),
        }
    }

    #[test]
    fn v1_17_chunks_are_followed_by_light_updates() {
        let mut iter = CircularChunkPacketIterator::new(
            (0, 0),
            0,
            None,
            1,
            &dimension_info(),
            ProtocolVersion::V1_17,
        );

        assert!(matches!(
            iter.next(),
            Some(PacketRegistry::ChunkDataAndUpdateLight(_))
        ));
        assert!(matches!(iter.next(), Some(PacketRegistry::LightUpdate(_))));
        assert!(iter.next().is_none());
    }

    #[test]
    fn pre_v1_16_chunks_are_sent_without_light_update_packets() {
        for version in [ProtocolVersion::V1_8, ProtocolVersion::V1_12_2] {
            let mut iter =
                CircularChunkPacketIterator::new((0, 0), 0, None, 1, &dimension_info(), version);

            assert!(matches!(
                iter.next(),
                Some(PacketRegistry::ChunkDataAndUpdateLight(_))
            ));
            assert!(
                iter.next().is_none(),
                "unexpected extra packet for {version:?}"
            );
        }
    }

    #[test]
    fn v1_14_and_v1_15_chunks_are_followed_by_light_updates() {
        for version in [ProtocolVersion::V1_14_4, ProtocolVersion::V1_15_2] {
            let mut iter =
                CircularChunkPacketIterator::new((0, 0), 0, None, 1, &dimension_info(), version);

            assert!(matches!(
                iter.next(),
                Some(PacketRegistry::ChunkDataAndUpdateLight(_))
            ));
            assert!(
                matches!(iter.next(), Some(PacketRegistry::LightUpdate(_))),
                "missing light update for {version:?}"
            );
            assert!(iter.next().is_none());
        }
    }

    #[test]
    fn v1_18_chunks_keep_light_in_chunk_packet() {
        let mut iter = CircularChunkPacketIterator::new(
            (0, 0),
            0,
            None,
            1,
            &dimension_info(),
            ProtocolVersion::V1_18,
        );

        assert!(matches!(
            iter.next(),
            Some(PacketRegistry::ChunkDataAndUpdateLight(_))
        ));
        assert!(iter.next().is_none());
    }
}
