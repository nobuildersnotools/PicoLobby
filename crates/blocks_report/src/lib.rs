use minecraft_protocol::prelude::{BinaryReader, BinaryReaderError, DecodePacket, ProtocolVersion};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub use blocks_report_data::{
    block_state_builder::BlockStateLookup,
    internal_mapping::{InternalId, InternalMapping, StateData},
    report_mapping::{BlocksReportId, ReportIdMapping},
};

include!(concat!(env!("OUT_DIR"), "/get_blocks_reports.rs"));
include!(concat!(env!("OUT_DIR"), "/block_entity_lookup.rs"));

static INTERNAL_MAPPING_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/internal_mapping"));

static BLOCK_REPORT_ID_MAPPINGS: LazyLock<
    Mutex<std::collections::HashMap<ProtocolVersion, Arc<[BlocksReportId]>>>,
> = LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

#[derive(Debug, Error)]
pub enum BlockReportIdMappingError {
    #[error("Failed to read binary data: {0}")]
    BinaryReader(#[from] BinaryReaderError),
    #[error("Protocol version {0} is not supported")]
    UnsupportedVersion(ProtocolVersion),
}

pub fn load_internal_mapping() -> Result<InternalMapping, BinaryReaderError> {
    let mut reader = BinaryReader::new(INTERNAL_MAPPING_DATA);
    InternalMapping::decode(&mut reader, ProtocolVersion::latest())
}

pub fn get_block_report_id_mapping(
    protocol_version: ProtocolVersion,
) -> Result<Arc<[BlocksReportId]>, BlockReportIdMappingError> {
    let mut mappings = BLOCK_REPORT_ID_MAPPINGS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(mapping) = mappings.get(&protocol_version) {
        return Ok(Arc::clone(mapping));
    }

    let mapping: Arc<[BlocksReportId]> = get_blocks_reports(protocol_version)?.into_inner().into();
    mappings.insert(protocol_version, Arc::clone(&mapping));
    Ok(mapping)
}

pub fn get_block_id(
    report_mapping: &[BlocksReportId],
    internal_id: InternalId,
) -> Option<BlocksReportId> {
    report_mapping.get(internal_id as usize).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolve a block-state string to the native block id for a protocol version,
    /// exercising the full schematic-string → InternalId → ViaVersion table path.
    fn native_id(state: &str, version: ProtocolVersion) -> BlocksReportId {
        let mapping = load_internal_mapping().expect("internal mapping");
        let lookup = BlockStateLookup::new(&mapping);
        let internal_id = lookup
            .parse_state_string(state)
            .expect("known block state")
            .internal_id();
        let report = get_block_report_id_mapping(version).expect("report mapping");
        get_block_id(&report, internal_id).expect("internal id in range")
    }

    #[test]
    fn pre_flattening_ids_are_block_id_times_16_plus_meta() {
        // 1.7-1.12 share the 1.12 numeric table: raw = block_id * 16 + metadata.
        assert_eq!(native_id("minecraft:air", ProtocolVersion::V1_12_2), 0);
        assert_eq!(native_id("minecraft:stone", ProtocolVersion::V1_12_2), 16); // 1<<4
        assert_eq!(native_id("minecraft:granite", ProtocolVersion::V1_12_2), 17); // 1<<4|1
        assert_eq!(native_id("minecraft:dirt", ProtocolVersion::V1_12_2), 48); // 3<<4
        assert_eq!(
            native_id("minecraft:white_wool", ProtocolVersion::V1_12_2),
            560
        ); // 35<<4
        assert_eq!(
            native_id("minecraft:red_wool", ProtocolVersion::V1_12_2),
            574
        ); // 35<<4|14
        assert_eq!(
            native_id("minecraft:white_concrete", ProtocolVersion::V1_12_2),
            4016 // 251<<4
        );
        // 1.7 uses the same table.
        assert_eq!(native_id("minecraft:stone", ProtocolVersion::V1_7_2), 16);
    }

    #[test]
    fn flattened_ids_match_each_version_natively() {
        // Stone is block-state id 1 across the flattened era; air is 0 everywhere.
        for version in [
            ProtocolVersion::V1_13,
            ProtocolVersion::V1_14,
            ProtocolVersion::V1_16,
            ProtocolVersion::V1_21,
            ProtocolVersion::V26_2,
        ] {
            assert_eq!(native_id("minecraft:air", version), 0, "air on {version:?}");
            assert_eq!(
                native_id("minecraft:stone", version),
                1,
                "stone on {version:?}"
            );
        }
    }

    #[test]
    fn anchor_version_maps_nearly_all_states() {
        // On the newest version (the resolution anchor) almost every InternalId
        // should resolve to a real native id; a low ratio means the canonical
        // identifier format drifted from the generator's.
        let mapping = load_internal_mapping().expect("internal mapping");
        let report = get_block_report_id_mapping(ProtocolVersion::V26_2).expect("report mapping");
        let (mut total, mut non_air) = (0u32, 0u32);
        for block in mapping.mapping.inner() {
            for state in block.states.inner() {
                total += 1;
                if report[state.state_data.internal_id() as usize] != 0 {
                    non_air += 1;
                }
            }
        }
        let ratio = f64::from(non_air) / f64::from(total);
        assert!(
            ratio > 0.97,
            "only {non_air}/{total} states mapped ({ratio:.3})"
        );
    }

    /// Blocks added after a client version must downgrade to the stand-in
    /// ViaVersion picks for them. These all used to resolve to air because the
    /// generator dropped the properties from Via's `name[` wildcard rewrites.
    #[test]
    fn newer_only_stairs_downgrade_instead_of_vanishing() {
        const STAIRS: [&str; 8] = [
            "minecraft:andesite_stairs",
            "minecraft:polished_andesite_stairs",
            "minecraft:granite_stairs",
            "minecraft:polished_granite_stairs",
            "minecraft:diorite_stairs",
            "minecraft:mossy_cobblestone_stairs",
            "minecraft:smooth_quartz_stairs",
            "minecraft:end_stone_brick_stairs",
        ];
        for version in [
            ProtocolVersion::V1_7_2,
            ProtocolVersion::V1_8,
            ProtocolVersion::V1_12_2,
            ProtocolVersion::V1_13,
            ProtocolVersion::V1_13_2,
            ProtocolVersion::V1_16,
        ] {
            for block in STAIRS {
                for state in [
                    format!("{block}[facing=east,half=bottom,shape=straight,waterlogged=false]"),
                    format!("{block}[facing=north,half=top,shape=inner_left,waterlogged=false]"),
                ] {
                    assert_ne!(
                        native_id(&state, version),
                        0,
                        "{state} became air on {version:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_supported_version_has_a_mapping() {
        // The V1_16 cross-version pivot is gone: each version resolves directly.
        for version in [
            ProtocolVersion::V1_7_2,
            ProtocolVersion::V1_8,
            ProtocolVersion::V1_13,
            ProtocolVersion::V1_15_2,
            ProtocolVersion::V1_20_2,
            ProtocolVersion::V26_2,
        ] {
            assert!(
                get_block_report_id_mapping(version).is_ok(),
                "missing mapping for {version:?}"
            );
        }
    }
}
