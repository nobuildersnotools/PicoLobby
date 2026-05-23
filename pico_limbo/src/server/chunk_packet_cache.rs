use crate::server::packet_registry::{PacketRegistry, PacketRegistryEncodeError};
use minecraft_protocol::prelude::{Dimension, ProtocolVersion};
use net::raw_packet::RawPacket;
use pico_registries::registry_provider::DimensionInfo;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ChunkPacketCacheKey {
    protocol_version: ProtocolVersion,
    view_distance: i32,
    center_chunk: (i32, i32),
    dimension: i8,
    biome_id: i32,
    dimension_height: i32,
    dimension_min_y: i32,
    dimension_protocol_id: u32,
    dimension_registry_key: String,
}

impl ChunkPacketCacheKey {
    pub fn new(
        protocol_version: ProtocolVersion,
        view_distance: i32,
        center_chunk: (i32, i32),
        dimension: Dimension,
        biome_id: i32,
        dimension_info: &DimensionInfo,
    ) -> Self {
        Self {
            protocol_version,
            view_distance,
            center_chunk,
            dimension: dimension.legacy_i8(),
            biome_id,
            dimension_height: dimension_info.height,
            dimension_min_y: dimension_info.min_y,
            dimension_protocol_id: dimension_info.protocol_id,
            dimension_registry_key: dimension_info.registry_key.to_string(),
        }
    }
}

type CachedChunkPackets = Arc<[RawPacket]>;
type CacheCell = OnceLock<Result<CachedChunkPackets, String>>;

#[derive(Default)]
pub struct ChunkPacketCache {
    packets: RwLock<HashMap<ChunkPacketCacheKey, Arc<CacheCell>>>,
}

impl ChunkPacketCache {
    pub fn get_or_encode<I, F>(
        &self,
        key: ChunkPacketCacheKey,
        protocol_version: ProtocolVersion,
        build_packets: F,
    ) -> Result<CachedChunkPackets, String>
    where
        I: IntoIterator<Item = PacketRegistry>,
        F: FnOnce() -> I,
    {
        let cell = self.get_or_create_cell(key);
        match cell.get_or_init(|| encode_packets(protocol_version, build_packets())) {
            Ok(packets) => Ok(Arc::clone(packets)),
            Err(err) => Err(err.clone()),
        }
    }

    fn get_or_create_cell(&self, key: ChunkPacketCacheKey) -> Arc<CacheCell> {
        if let Some(cell) = self
            .packets
            .read()
            .ok()
            .and_then(|cache| cache.get(&key).cloned())
        {
            return cell;
        }

        let mut cache = match self.packets.write() {
            Ok(cache) => cache,
            Err(poisoned) => poisoned.into_inner(),
        };
        Arc::clone(cache.entry(key).or_default())
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.packets.read().map_or(0, |cache| cache.len())
    }
}

fn encode_packets<I>(
    protocol_version: ProtocolVersion,
    packets: I,
) -> Result<CachedChunkPackets, String>
where
    I: IntoIterator<Item = PacketRegistry>,
{
    packets
        .into_iter()
        .map(|packet| {
            packet
                .encode_packet(protocol_version)
                .map_err(|err: PacketRegistryEncodeError| err.to_string())
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::play::send_chunks_circularly::CircularChunkPacketIterator;
    use pico_registries::Identifier;

    fn dimension_info() -> DimensionInfo {
        DimensionInfo {
            height: 256,
            min_y: 0,
            protocol_id: 0,
            registry_key: Identifier::vanilla_unchecked("overworld"),
        }
    }

    fn key(protocol_version: ProtocolVersion) -> ChunkPacketCacheKey {
        ChunkPacketCacheKey::new(
            protocol_version,
            0,
            (0, 0),
            Dimension::Overworld,
            1,
            &dimension_info(),
        )
    }

    fn iterator(protocol_version: ProtocolVersion) -> CircularChunkPacketIterator {
        CircularChunkPacketIterator::new((0, 0), 0, None, 1, &dimension_info(), protocol_version)
    }

    #[test]
    fn cache_matches_uncached_packet_order() {
        let cache = ChunkPacketCache::default();
        let protocol_version = ProtocolVersion::V1_20_5;

        let cached = cache
            .get_or_encode(key(protocol_version), protocol_version, || {
                iterator(protocol_version)
            })
            .unwrap();
        let uncached = encode_packets(protocol_version, iterator(protocol_version)).unwrap();

        assert_eq!(cached.len(), uncached.len());
        assert_eq!(
            cached.iter().map(RawPacket::bytes).collect::<Vec<&[u8]>>(),
            uncached
                .iter()
                .map(RawPacket::bytes)
                .collect::<Vec<&[u8]>>()
        );
    }

    #[test]
    fn v1_17_cache_includes_legacy_light_update_packet() {
        let cache = ChunkPacketCache::default();
        let protocol_version = ProtocolVersion::V1_17;

        let cached = cache
            .get_or_encode(key(protocol_version), protocol_version, || {
                iterator(protocol_version)
            })
            .unwrap();

        assert_eq!(cached.len(), 2);
    }

    #[test]
    fn protocol_versions_do_not_share_cached_packets() {
        let cache = ChunkPacketCache::default();

        let v1_20_5 = cache
            .get_or_encode(
                key(ProtocolVersion::V1_20_5),
                ProtocolVersion::V1_20_5,
                || iterator(ProtocolVersion::V1_20_5),
            )
            .unwrap();
        let v1_19 = cache
            .get_or_encode(key(ProtocolVersion::V1_19), ProtocolVersion::V1_19, || {
                iterator(ProtocolVersion::V1_19)
            })
            .unwrap();

        assert_eq!(cache.len(), 2);
        assert_ne!(v1_20_5[0].bytes(), v1_19[0].bytes());
    }
}
