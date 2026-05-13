use minecraft_protocol::prelude::*;

/// Serverbound legacy item use/block placement packet used by 1.8.x and below.
///
/// Older clients send this packet for right-click item interactions instead of
/// the 1.9+ `minecraft:use_item` packet. PicoLobby only needs the interaction
/// signal; the target position and carried item snapshot are ignored.
pub struct LegacyUseItemPacket;

impl DecodePacket for LegacyUseItemPacket {
    fn decode(
        _reader: &mut BinaryReader,
        _version: ProtocolVersion,
    ) -> Result<Self, BinaryReaderError> {
        Ok(Self)
    }
}
