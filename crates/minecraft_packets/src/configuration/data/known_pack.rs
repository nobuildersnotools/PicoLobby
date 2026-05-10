use minecraft_protocol::prelude::*;

#[derive(PacketIn, PacketOut)]
pub struct KnownPack {
    namespace: String,
    id: String,
    version: String,
}

impl KnownPack {
    pub fn new(version: &str) -> Self {
        Self {
            namespace: "minecraft".to_string(),
            id: "core".to_string(),
            version: version.to_string(),
        }
    }

    #[must_use]
    pub fn is_minecraft_core(&self, version: &str) -> bool {
        self.namespace == "minecraft" && self.id == "core" && self.version == version
    }
}
