use crate::blocks_report_loader::BlocksReport;
use blocks_report_data::legacy_mapping::{LegacyBlockMapping, LegacyEntry};
use minecraft_protocol::prelude::LengthPaddedVec;
use protocol_version::protocol_version::ProtocolVersion;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct LegacyJsonEntry {
    pub name: String,
    #[serde(default)]
    pub required_props: HashMap<String, String>,
    pub id: u8,
    pub meta: u8,
}

pub fn load_legacy_json() -> anyhow::Result<Vec<LegacyJsonEntry>> {
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("data")
        .join("generated")
        .join("Any")
        .join("legacy_block_mapping.json");

    let content = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn build_legacy_mapping(
    blocks_reports: &[BlocksReport],
    legacy_entries: &[LegacyJsonEntry],
) -> LegacyBlockMapping {
    let v1_16 = blocks_reports
        .iter()
        .find(|r| r.protocol_version == ProtocolVersion::V1_16)
        .expect("V1_16 blocks report is required for legacy mapping");

    // Find the maximum state ID in V1_16
    let max_state_id = v1_16
        .block_data
        .blocks
        .values()
        .flat_map(|block| block.states.iter().map(|s| s.id as usize))
        .max()
        .unwrap_or(0);

    let mut mapping: Vec<LegacyEntry> = vec![LegacyEntry::NO_MAPPING; max_state_id + 1];

    for (block_name, block) in &v1_16.block_data.blocks {
        for state in &block.states {
            let state_props = state.properties.as_ref();

            // Find the first matching legacy entry for this (name, properties) combo
            let matched = legacy_entries.iter().find(|entry| {
                if entry.name != *block_name {
                    return false;
                }
                // All required_props must appear with matching values in the state's properties
                entry.required_props.iter().all(|(req_key, req_val)| {
                    state_props
                        .and_then(|p| p.get(req_key.as_str()))
                        .map(|v| v == req_val)
                        .unwrap_or(false)
                })
            });

            if let Some(entry) = matched {
                mapping[state.id as usize] = LegacyEntry {
                    block_id: entry.id,
                    metadata: entry.meta,
                };
            }
        }
    }

    LengthPaddedVec::new(mapping)
}
