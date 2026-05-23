use minecraft_protocol::prelude::{BinaryReader, BinaryReaderError, DecodePacket, ProtocolVersion};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub use blocks_report_data::{
    block_state_builder::BlockStateLookup,
    internal_mapping::{InternalId, InternalMapping, StateData},
    legacy_mapping::{LegacyBlockMapping, LegacyEntry},
    report_mapping::{BlocksReportId, ReportIdMapping},
};

include!(concat!(env!("OUT_DIR"), "/get_blocks_reports.rs"));
include!(concat!(env!("OUT_DIR"), "/block_entity_lookup.rs"));

static INTERNAL_MAPPING_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/internal_mapping"));
static LEGACY_BLOCK_MAPPING_DATA: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/legacy_block_mapping"));

static LEGACY_BLOCK_MAPPING: LazyLock<Vec<LegacyEntry>> = LazyLock::new(|| {
    let mut reader = BinaryReader::new(LEGACY_BLOCK_MAPPING_DATA);
    LegacyBlockMapping::decode(&mut reader, ProtocolVersion::latest())
        .map(|m| m.into_inner())
        .unwrap_or_default()
});
static BLOCK_REPORT_ID_MAPPINGS: LazyLock<
    Mutex<std::collections::HashMap<ProtocolVersion, Arc<[BlocksReportId]>>>,
> = LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

pub fn get_legacy_block_mapping() -> &'static [LegacyEntry] {
    &LEGACY_BLOCK_MAPPING
}

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
