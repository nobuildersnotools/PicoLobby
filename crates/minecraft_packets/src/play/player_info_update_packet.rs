use crate::login::Property;
use minecraft_protocol::prelude::*;
use pico_text_component::prelude::Component;

pub struct PlayerInfoUpdatePacket {
    action: VarInt,
    v1_19_3_mask: u8,
    players: LengthPaddedVec<Player>,
    kind: PlayerInfoUpdateKind,
}

impl PlayerInfoUpdatePacket {
    pub fn skin(name: String, uuid: Uuid, property: Property, listed: bool) -> Self {
        Self::new(name, uuid, vec![property], listed)
    }

    pub fn skinless(name: String, uuid: Uuid, listed: bool) -> Self {
        Self::new(name, uuid, Vec::new(), listed)
    }

    pub fn remove(uuid: Uuid, name: String) -> Self {
        let player_action = Player {
            uuid: uuid.into(),
            legacy_action: LegacyPlayerAction::RemovePlayer { name },
            actions: Vec::new(),
        };

        Self {
            action: VarInt::new(4),
            v1_19_3_mask: 0,
            players: LengthPaddedVec::new(vec![player_action]),
            kind: PlayerInfoUpdateKind::Remove,
        }
    }

    pub fn remove_legacy_name(name: String) -> Self {
        let player_action = Player {
            uuid: Uuid::nil().into(),
            legacy_action: LegacyPlayerAction::RemovePlayer { name },
            actions: Vec::new(),
        };

        Self {
            action: VarInt::new(4),
            v1_19_3_mask: 0,
            players: LengthPaddedVec::new(vec![player_action]),
            kind: PlayerInfoUpdateKind::Remove,
        }
    }

    fn new(name: String, uuid: Uuid, properties: Vec<Property>, listed: bool) -> Self {
        let properties = LengthPaddedVec::new(properties);
        let add_player_action = AddPlayer {
            name,
            properties,
            game_mode: VarInt::new(1),
            ping: VarInt::new(1),
            display_name: Optional::None,
            sig_data: Optional::None,
        };

        let actions = vec![
            PlayerActions::AddPlayer(add_player_action.clone()),
            PlayerActions::UpdateListed { listed },
        ];

        let mut mask = 0;
        for action in &actions {
            mask |= action.get_mask();
        }

        let player_action = Player {
            uuid: uuid.into(),
            legacy_action: LegacyPlayerAction::AddPlayer(add_player_action.clone()),
            actions,
        };

        Self {
            action: VarInt::new(0),
            v1_19_3_mask: mask,
            players: LengthPaddedVec::new(vec![player_action]),
            kind: PlayerInfoUpdateKind::Update,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum PlayerInfoUpdateKind {
    Update,
    Remove,
}

impl EncodePacket for PlayerInfoUpdatePacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_7_6) {
            for player in self.players.inner() {
                player.encode_legacy_v1_7(writer, protocol_version)?;
            }
            return Ok(());
        }

        if self.kind == PlayerInfoUpdateKind::Remove
            || protocol_version.is_before_inclusive(ProtocolVersion::V1_19_1)
        {
            self.action.encode(writer, protocol_version)?;
        } else {
            self.v1_19_3_mask.encode(writer, protocol_version)?;
        }
        self.players.encode(writer, protocol_version)
    }
}

struct Player {
    uuid: UuidAsLongs,
    legacy_action: LegacyPlayerAction,
    actions: Vec<PlayerActions>,
}

impl Player {
    fn encode_legacy_v1_7(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        match &self.legacy_action {
            LegacyPlayerAction::AddPlayer(value) => {
                value.name.encode(writer, protocol_version)?;
                true.encode(writer, protocol_version)?;
                1_i16.encode(writer, protocol_version)
            }
            LegacyPlayerAction::RemovePlayer { name } => {
                name.encode(writer, protocol_version)?;
                false.encode(writer, protocol_version)?;
                0_i16.encode(writer, protocol_version)
            }
        }
    }
}

impl EncodePacket for Player {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        self.uuid.encode(writer, protocol_version)?;
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_19_1) {
            self.legacy_action.encode(writer, protocol_version)
        } else {
            self.actions.encode(writer, protocol_version)
        }
    }
}

#[derive(PacketOut, Clone)]
struct AddPlayer {
    name: String,
    properties: LengthPaddedVec<Property>,
    #[pvn(..761)]
    game_mode: VarInt,
    #[pvn(..761)]
    ping: VarInt,
    #[pvn(..761)]
    display_name: Optional<Component>,
    #[pvn(759..761)]
    sig_data: Optional<SigData>,
}

#[derive(PacketOut, Clone)]
struct SigData {
    timestamp: i64,
    public_key: LengthPaddedVec<i8>,
    signature: LengthPaddedVec<i8>,
}

#[derive(Clone)]
enum LegacyPlayerAction {
    AddPlayer(AddPlayer),
    RemovePlayer { name: String },
}

impl EncodePacket for LegacyPlayerAction {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        match self {
            Self::AddPlayer(value) => value.encode(writer, protocol_version),
            Self::RemovePlayer { .. } => Ok(()),
        }
    }
}

#[derive(Clone)]
enum PlayerActions {
    AddPlayer(AddPlayer),
    UpdateListed { listed: bool },
}

impl PlayerActions {
    fn get_mask(&self) -> u8 {
        match self {
            PlayerActions::AddPlayer { .. } => 0x01,
            PlayerActions::UpdateListed { .. } => 0x08,
        }
    }
}

impl EncodePacket for PlayerActions {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        match self {
            PlayerActions::AddPlayer(value) => {
                value.encode(writer, protocol_version)?;
                Ok(())
            }
            PlayerActions::UpdateListed { listed } => {
                listed.encode(writer, protocol_version)?;
                Ok(())
            }
        }
    }
}
