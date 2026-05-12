use minecraft_protocol::prelude::*;

/// One entry in the compile-time legacy block mapping, indexed by V1_16 block state ID.
///
/// `metadata == 0xFF` is the sentinel meaning "no legacy equivalent; render as air".
/// Valid pre-flattening metadata is always 0–15.
#[derive(PacketOut, PacketIn, Copy, Clone)]
pub struct LegacyEntry {
    pub block_id: u8,
    pub metadata: u8,
}

impl LegacyEntry {
    pub const NO_MAPPING: Self = Self {
        block_id: 0,
        metadata: 0xFF,
    };

    pub const fn has_mapping(self) -> bool {
        self.metadata != 0xFF
    }
}

pub type LegacyBlockMapping = LengthPaddedVec<LegacyEntry>;
