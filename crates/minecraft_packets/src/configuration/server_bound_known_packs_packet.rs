use crate::configuration::data::known_pack::KnownPack;
use minecraft_protocol::prelude::*;

#[derive(PacketIn)]
pub struct ServerBoundKnownPacksPacket {
    known_packs: LengthPaddedVec<KnownPack>,
}

impl ServerBoundKnownPacksPacket {
    #[must_use]
    pub fn contains_minecraft_core(&self, version: &str) -> bool {
        self.known_packs
            .inner()
            .iter()
            .any(|known_pack| known_pack.is_minecraft_core(version))
    }
}
