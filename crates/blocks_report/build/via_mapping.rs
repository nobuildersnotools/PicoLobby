use blocks_report_data::internal_mapping::{InternalId, InternalMapping, InternalProperties};
use blocks_report_data::report_mapping::{BlocksReportId, ReportIdMapping};
use minecraft_protocol::prelude::LengthPaddedVec;
use protocol_version::protocol_version::ProtocolVersion;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::{env, fs};

/// One protocol version's `InternalId -> native block id` table. For 1.13+ the
/// value is the flat block-state id; for 1.7-1.12 it is the pre-Flattening
/// `block_id * 16 + metadata`. Unmapped internal ids fall back to air (0).
pub struct ReportMapping {
    pub protocol_version: ProtocolVersion,
    pub mapping: ReportIdMapping,
}

#[derive(Deserialize)]
struct IdentifiersFile {
    identifiers: Vec<String>,
}

#[derive(Deserialize)]
struct VersionsFile {
    versions: HashMap<String, String>,
}

#[derive(Deserialize)]
struct ViaTable {
    /// `ids[i]` is the native block id for `identifiers[i]`, or -1 if unmapped.
    ids: Vec<i32>,
}

fn via_dir() -> PathBuf {
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("data")
        .join("generated")
        .join("Any")
        .join("via")
}

/// The canonical identifier `minecraft:<name>[k1=v1,k2=v2]` for a block state,
/// with property keys already sorted (they are stored sorted in `InternalState`).
/// Must match byte-for-byte the keys emitted by
/// `data/src/generate_via_block_mappings.ts`.
pub fn canonical_identifier(name: &str, properties: &[InternalProperties]) -> String {
    if properties.is_empty() {
        return name.to_string();
    }
    let inner = properties
        .iter()
        .map(|p| format!("{}={}", p.name, p.value))
        .collect::<Vec<_>>()
        .join(",");
    format!("{name}[{inner}]")
}

/// Build an `InternalId -> native block id` table for every supported protocol
/// version from the committed ViaVersion-derived data under `data/generated/Any/via`.
pub fn build_via_report_mappings(
    internal_mapping: &InternalMapping,
) -> anyhow::Result<Vec<ReportMapping>> {
    let dir = via_dir();
    let identifiers: Vec<String> = serde_json::from_str::<IdentifiersFile>(&fs::read_to_string(
        dir.join("identifiers.json"),
    )?)?
    .identifiers;
    let versions: HashMap<String, String> =
        serde_json::from_str::<VersionsFile>(&fs::read_to_string(dir.join("versions.json"))?)?
            .versions;

    // canonical identifier -> InternalId, and the largest id (table length).
    let mut canon_to_internal: HashMap<String, InternalId> = HashMap::new();
    let mut max_internal_id: InternalId = 0;
    for block in internal_mapping.mapping.inner() {
        for state in block.states.inner() {
            let id = state.state_data.internal_id();
            max_internal_id = max_internal_id.max(id);
            canon_to_internal.insert(
                canonical_identifier(&block.name, state.properties.inner()),
                id,
            );
        }
    }
    let num_internal_ids = max_internal_id as usize + 1;

    // For each row of the shared identifier list, the InternalId it resolves to.
    let internal_for_row: Vec<Option<InternalId>> = identifiers
        .iter()
        .map(|ident| canon_to_internal.get(ident).copied())
        .collect();

    // Load each referenced via-version table once.
    let mut table_cache: HashMap<String, Vec<i32>> = HashMap::new();
    for via_version in versions.values() {
        if table_cache.contains_key(via_version) {
            continue;
        }
        let table: ViaTable = serde_json::from_str(&fs::read_to_string(
            dir.join(format!("{via_version}.json")),
        )?)?;
        table_cache.insert(via_version.clone(), table.ids);
    }

    let mut variants: Vec<&String> = versions.keys().collect();
    variants.sort();

    let mut mappings = Vec::with_capacity(variants.len());
    for variant in variants {
        let Ok(protocol_version) = ProtocolVersion::from_str(variant) else {
            continue;
        };
        let ids = &table_cache[&versions[variant]];

        let mut report_vec: Vec<BlocksReportId> = vec![0; num_internal_ids]; // air default
        for (row, maybe_internal) in internal_for_row.iter().enumerate() {
            if let Some(internal_id) = maybe_internal {
                let native = ids.get(row).copied().unwrap_or(-1);
                if native >= 0 {
                    report_vec[*internal_id as usize] = native as BlocksReportId;
                }
            }
        }

        mappings.push(ReportMapping {
            protocol_version,
            mapping: LengthPaddedVec::new(report_vec),
        });
    }

    Ok(mappings)
}
