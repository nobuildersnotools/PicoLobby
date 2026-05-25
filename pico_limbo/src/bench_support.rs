use crate::configuration::config::Config;
use crate::configuration::lobby::{LobbyNpcConfig, LobbyServerEntry};
use crate::handlers::play::send_chunks_circularly::CircularChunkPacketIterator;
use crate::server::batch::{Batch, OutboundPacket};
use crate::server::chunk_packet_cache::{ChunkPacketCache, ChunkPacketCacheKey};
use crate::server::lobby_chat::{
    chat_component_for_plan, escape_minimessage_text, private_message_packets_for_plan,
};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{
    EntityId, LobbyChatPlan, LobbyPrivateMessagePlan, LobbyRecipient, LobbySessionId,
};
use futures::StreamExt;
use minecraft_packets::play::move_entity_packet::{MoveEntityPosPacket, RelativeMoveDelta};
use minecraft_packets::play::remove_entities_packet::RemoveEntitiesPacket;
use minecraft_packets::play::scoreboard_packets::{
    SetDisplayObjectivePacket, SetObjectivePacket, SetPlayerTeamPacket, SetScorePacket,
};
use minecraft_packets::play::set_container_slot_packet::SetContainerSlotPacket;
use minecraft_packets::play::system_chat_message_packet::SystemChatMessagePacket;
use minecraft_packets::play::{LobbySlot, VoidChunkContext};
use minecraft_protocol::prelude::{Dimension, ProtocolVersion, State, Uuid};
use net::raw_packet::RawPacket;
use pico_nbt::{IndexMap, Value};
use pico_registries::Identifier;
use pico_registries::registry_provider::DimensionInfo;
use pico_text_component::prelude::{Component, parse_mini_message};
use std::sync::Arc;

#[derive(Copy, Clone)]
pub enum BenchProtocol {
    Legacy,
    LightUpdate,
    Modern,
    Latest,
}

impl BenchProtocol {
    const fn version(self) -> ProtocolVersion {
        match self {
            Self::Legacy => ProtocolVersion::V1_8,
            Self::LightUpdate => ProtocolVersion::V1_17,
            Self::Modern => ProtocolVersion::V1_20_5,
            Self::Latest => ProtocolVersion::V26_1,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Legacy => "v1_8",
            Self::LightUpdate => "v1_17",
            Self::Modern => "v1_20_5",
            Self::Latest => "v26_1",
        }
    }
}

pub const PROTOCOLS: [BenchProtocol; 4] = [
    BenchProtocol::Legacy,
    BenchProtocol::LightUpdate,
    BenchProtocol::Modern,
    BenchProtocol::Latest,
];

pub const VIEW_DISTANCES: [i32; 3] = [0, 2, 4];

pub struct HotChunkCache {
    cache: ChunkPacketCache,
    key: ChunkPacketCacheKey,
    version: ProtocolVersion,
    view_distance: i32,
}

impl HotChunkCache {
    pub fn new(protocol: BenchProtocol, view_distance: i32) -> Self {
        let version = protocol.version();
        let cache = ChunkPacketCache::default();
        let key = chunk_cache_key(version, view_distance);
        let _ = cache
            .get_or_encode(key.clone(), version, || {
                chunk_iterator(version, view_distance)
            })
            .expect("warm chunk cache");
        Self {
            cache,
            key,
            version,
            view_distance,
        }
    }

    pub fn get(&self) -> usize {
        let packets = match self.cache.get_cached(&self.key) {
            Some(packets) => packets.expect("read hot chunk cache"),
            None => self
                .cache
                .get_or_encode(self.key.clone(), self.version, || {
                    chunk_iterator(self.version, self.view_distance)
                })
                .expect("read hot chunk cache"),
        };
        cached_packet_bytes(&packets)
    }
}

pub fn collect_chunk_packets(protocol: BenchProtocol, view_distance: i32) -> usize {
    chunk_iterator(protocol.version(), view_distance).count()
}

pub fn encode_chunk_packets(protocol: BenchProtocol, view_distance: i32) -> usize {
    let version = protocol.version();
    chunk_iterator(version, view_distance)
        .map(|packet| {
            packet
                .encode_packet(version)
                .expect("encode chunk packet")
                .size()
        })
        .sum()
}

pub fn chunk_cache_cold(protocol: BenchProtocol, view_distance: i32) -> usize {
    let version = protocol.version();
    let cache = ChunkPacketCache::default();
    let packets = cache
        .get_or_encode(chunk_cache_key(version, view_distance), version, || {
            chunk_iterator(version, view_distance)
        })
        .expect("encode cold chunk cache");
    cached_packet_bytes(&packets)
}

pub fn chunk_cache_hot(protocol: BenchProtocol, view_distance: i32) -> usize {
    HotChunkCache::new(protocol, view_distance).get()
}

pub async fn drain_mixed_batch() -> usize {
    let mut batch = Batch::new();
    batch.push_item(chat_packet("direct"));
    batch.queue(movement_packet);
    batch.queue_async(|| async { scoreboard_packet() });
    batch.chain_iter(vec![selector_slot_packet(), chunk_packet()]);

    drain_batch(batch).await
}

pub async fn drain_raw_cache_batch() -> usize {
    let version = ProtocolVersion::V1_20_5;
    let cache = ChunkPacketCache::default();
    let packets = cache
        .get_or_encode(chunk_cache_key(version, 2), version, || {
            chunk_iterator(version, 2)
        })
        .expect("build raw packet cache");
    let mut batch = Batch::new();
    batch.chain_raw_packet_cache(packets);

    drain_batch(batch).await
}

pub fn encode_representative_packets(protocol: BenchProtocol) -> usize {
    let version = protocol.version();
    representative_packets(version)
        .into_iter()
        .map(|packet| packet.encode_packet(version).expect("encode packet").size())
        .sum()
}

pub fn decode_representative_packets() -> usize {
    decode_fixtures()
        .into_iter()
        .map(|(version, raw_packet)| {
            let packet =
                PacketRegistry::decode_packet(version, State::Play, raw_packet).expect("decode");
            decoded_packet_weight(packet)
        })
        .sum()
}

pub fn format_lobby_chat() -> usize {
    chat_component_for_plan(&chat_plan()).to_json().len()
}

pub fn private_message_packets() -> usize {
    private_message_packets_for_plan(&private_message_plan()).len()
}

pub fn escape_minimessage() -> usize {
    escape_minimessage_text("<red>Alice & Bob</red> > lobby").len()
}

pub fn component_json() -> usize {
    component_fixture().to_json().len()
}

pub fn component_nbt() -> usize {
    component_fixture().to_nbt().id() as usize
}

pub fn component_legacy() -> usize {
    component_fixture().to_legacy_text().len()
}

pub fn nbt_to_bytes() -> usize {
    pico_nbt::to_bytes(&nbt_fixture(), Some("root"))
        .expect("encode nbt")
        .len()
}

pub fn nbt_from_slice() -> usize {
    let bytes = pico_nbt::to_bytes(&nbt_fixture(), Some("root")).expect("encode nbt");
    let (name, value) = pico_nbt::from_slice(&bytes).expect("decode nbt");
    name.len() + value.id() as usize
}

pub fn default_config_parse() -> usize {
    let toml = toml::to_string_pretty(&Config::default()).expect("serialize default config");
    let config: Config = toml::from_str(&toml).expect("parse default config");
    config.lobby.servers.len()
}

pub fn lobby_heavy_config_parse() -> usize {
    let toml = lobby_heavy_config_toml();
    let config: Config = toml::from_str(&toml).expect("parse lobby-heavy config");
    config.lobby.servers.len() + config.lobby.npcs.len()
}

fn chunk_iterator(version: ProtocolVersion, view_distance: i32) -> CircularChunkPacketIterator {
    CircularChunkPacketIterator::new((0, 0), view_distance, None, 1, &dimension_info(), version)
}

fn chunk_cache_key(version: ProtocolVersion, view_distance: i32) -> ChunkPacketCacheKey {
    ChunkPacketCacheKey::new(
        version,
        view_distance,
        (0, 0),
        Dimension::Overworld,
        1,
        &dimension_info(),
    )
}

fn dimension_info() -> DimensionInfo {
    DimensionInfo {
        height: 256,
        min_y: 0,
        protocol_id: 0,
        registry_key: Identifier::vanilla_unchecked("overworld"),
    }
}

fn cached_packet_bytes(packets: &Arc<[RawPacket]>) -> usize {
    packets.iter().map(RawPacket::size).sum()
}

async fn drain_batch(batch: Batch<PacketRegistry>) -> usize {
    let mut stream = batch.into_outbound_stream();
    let mut total = 0;
    while let Some(packet) = stream.next().await {
        total += match packet {
            OutboundPacket::Registry(packet) => packet
                .encode_packet(ProtocolVersion::V1_20_5)
                .expect("encode batch packet")
                .size(),
            OutboundPacket::Raw(packet) => packet.size(),
        };
    }
    total
}

fn representative_packets(version: ProtocolVersion) -> Vec<PacketRegistry> {
    let component = component_fixture();
    let mut packets = vec![
        crate::server::lobby_chat::chat_packet_for_version(version, &component),
        movement_packet(),
        scoreboard_packet(),
        PacketRegistry::SetDisplayObjective(SetDisplayObjectivePacket::sidebar("picolobby")),
        PacketRegistry::SetPlayerTeam(SetPlayerTeamPacket::create(
            "plsb00",
            Component::new(""),
            Component::new("Line"),
            Component::new(""),
            vec!["\u{00a7}0".to_string()],
        )),
        PacketRegistry::SetScore(SetScorePacket::change("\u{00a7}0", "picolobby", 1)),
        selector_slot_packet(),
        chunk_packet(),
    ];

    if version.is_after_inclusive(ProtocolVersion::V1_19) {
        packets.push(PacketRegistry::SystemChatMessage(
            SystemChatMessagePacket::component(&component),
        ));
    }

    if version.is_after_inclusive(ProtocolVersion::V26_1) {
        packets.push(PacketRegistry::RemoveEntities(
            RemoveEntitiesPacket::single(300),
        ));
    }

    packets
}

fn chat_packet(message: &str) -> PacketRegistry {
    crate::server::lobby_chat::chat_packet_for_version(
        ProtocolVersion::V1_20_5,
        &Component::new(message),
    )
}

fn movement_packet() -> PacketRegistry {
    let delta = RelativeMoveDelta::new_unchecked(2048, -1024, 512);
    PacketRegistry::MoveEntityPos(MoveEntityPosPacket::new(300, delta, true))
}

fn scoreboard_packet() -> PacketRegistry {
    PacketRegistry::SetObjective(SetObjectivePacket::create(
        "picolobby",
        Component::new("PicoLobby"),
    ))
}

fn selector_slot_packet() -> PacketRegistry {
    PacketRegistry::SetContainerSlot(SetContainerSlotPacket::hotbar(
        4,
        LobbySlot::new(345, 1, None, Vec::new()),
    ))
}

fn chunk_packet() -> PacketRegistry {
    let context = VoidChunkContext {
        chunk_x: 0,
        chunk_z: 0,
        biome_index: 1,
        dimension_height: 256,
        dimension_min_y: 0,
    };
    PacketRegistry::ChunkDataAndUpdateLight(Box::new(
        minecraft_packets::play::chunk_data_and_update_light_packet::ChunkDataAndUpdateLightPacket::void(
            context,
        ),
    ))
}

fn decode_fixtures() -> Vec<(ProtocolVersion, RawPacket)> {
    vec![
        (
            ProtocolVersion::V1_20_5,
            RawPacket::from_bytes(0x2f, &[0x00, 0x04]),
        ),
        (
            ProtocolVersion::V1_21,
            RawPacket::from_bytes(22, &[0xac, 0x02, 0, 0, 0]),
        ),
        (
            ProtocolVersion::V26_1,
            RawPacket::from_bytes(1, &[0xac, 0x02]),
        ),
        (
            ProtocolVersion::V1_21,
            RawPacket::from_bytes(37, &[0xac, 0x02, 0, 0]),
        ),
        (ProtocolVersion::V26_1, RawPacket::from_bytes(43, &[0x20])),
        (
            ProtocolVersion::V1_21,
            RawPacket::from_bytes(0x1c, &[0x42, 0xb4, 0, 0, 0, 0, 0, 0, 1]),
        ),
    ]
}

fn decoded_packet_weight(packet: PacketRegistry) -> usize {
    match packet {
        PacketRegistry::ServerBoundSetHeldItem(packet) => packet.selected_slot() as usize,
        PacketRegistry::Interact(packet) => packet.target_entity_id() as usize,
        PacketRegistry::Attack(packet) => packet.target_entity_id() as usize,
        PacketRegistry::PlayerCommand(packet) => packet.entity_id() as usize,
        PacketRegistry::PlayerInput(packet) => usize::from(packet.shift().unwrap_or(false)),
        PacketRegistry::SetPlayerRotation(_) => 1,
        _ => 0,
    }
}

fn chat_plan() -> LobbyChatPlan {
    LobbyChatPlan {
        sender_session_id: LobbySessionId::new(1),
        sender_username: "BenchPlayer".to_string(),
        message: "Hello <red>world</red> & everyone".to_string(),
        format: "<white>&lt;{sender}&gt; {message}</white>".to_string(),
        recipients: vec![recipient(1, ProtocolVersion::V1_20_5)],
    }
}

fn private_message_plan() -> LobbyPrivateMessagePlan {
    LobbyPrivateMessagePlan {
        sender_session_id: LobbySessionId::new(1),
        recipient_session_id: LobbySessionId::new(2),
        sender_username: "Sender".to_string(),
        recipient_username: "Recipient".to_string(),
        message: "hello <gold>there</gold>".to_string(),
        sender_format: "<gray>[me -> {recipient}]</gray> <white>{message}</white>".to_string(),
        recipient_format: "<gray>[{sender} -> me]</gray> <white>{message}</white>".to_string(),
        sender_recipient: recipient(1, ProtocolVersion::V1_18_2),
        message_recipient: recipient(2, ProtocolVersion::V1_20_5),
    }
}

fn recipient(id: u64, version: ProtocolVersion) -> LobbyRecipient {
    LobbyRecipient {
        session_id: LobbySessionId::new(id),
        uuid: Uuid::from_u128(id as u128),
        entity_id: EntityId::new(id as i32),
        protocol_version: version,
    }
}

fn component_fixture() -> Component {
    parse_mini_message(
        "<gray>[Lobby]</gray> <white>Welcome, <green>BenchPlayer</green></white><newline/><yellow>Choose a server.</yellow>",
    )
    .expect("parse component fixture")
}

fn nbt_fixture() -> Value {
    let mut root = IndexMap::new();
    root.insert("name".to_string(), Value::String("PicoLobby".to_string()));
    root.insert("enabled".to_string(), Value::Byte(1));
    root.insert("spawn".to_string(), Value::IntArray(vec![0, 320, 0]));
    root.insert("samples".to_string(), Value::LongArray((0..64).collect()));

    let mut nested = IndexMap::new();
    nested.insert(
        "motd".to_string(),
        Value::String("Benchmark lobby".to_string()),
    );
    nested.insert("players".to_string(), Value::Int(42));
    nested.insert(
        "servers".to_string(),
        Value::List(vec![
            Value::String("survival".to_string()),
            Value::String("creative".to_string()),
            Value::String("minigames".to_string()),
        ]),
    );
    root.insert("lobby".to_string(), Value::Compound(nested));

    Value::Compound(root)
}

fn lobby_heavy_config_toml() -> String {
    let mut config = Config::default();
    config.lobby.enabled = true;
    config.lobby.servers = (0..16)
        .map(|index| LobbyServerEntry {
            id: format!("server-{index}"),
            display_name: format!("Server {index}"),
            server: format!("server-{index}"),
        })
        .collect();
    config.lobby.npcs = (0..16)
        .map(|index| LobbyNpcConfig {
            id: format!("npc-{index}"),
            destination: format!("server-{index}"),
            name: format!("Server {index}"),
            x: f64::from(index),
            y: 320.0,
            z: f64::from(index * 2),
            yaw: 180.0,
            pitch: 0.0,
        })
        .collect();
    config.scoreboard.lines = (0..15)
        .map(|index| format!("<gray>Line {index}: <green>{{online}}</green>"))
        .collect();
    toml::to_string_pretty(&config).expect("serialize lobby-heavy config")
}
