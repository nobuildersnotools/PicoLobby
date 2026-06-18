use crate::handlers::play::fetch_minecraft_profile::fetch_minecraft_profile;
use crate::handlers::play::send_chunks_circularly::CircularChunkPacketIterator;
use crate::server::batch::Batch;
use crate::server::chunk_packet_cache::ChunkPacketCacheKey;
use crate::server::client_state::ClientState;
use crate::server::game_mode::GameMode;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_brand::SERVER_BRAND;
use crate::server_state::{
    ConfigPlaceholders, RenderedScoreboard, Scoreboard, ScoreboardPlaceholders, ServerCommand,
    ServerState, TabList, Title, TitleType,
};
use minecraft_packets::configuration::acknowledge_finish_configuration_packet::AcknowledgeConfigurationPacket;
use minecraft_packets::login::Property;
use minecraft_packets::play::boss_bar_packet::BossBarPacket;
use minecraft_packets::play::client_bound_player_abilities_packet::ClientBoundPlayerAbilitiesPacket;
use minecraft_packets::play::client_bound_plugin_message_packet::PlayClientBoundPluginMessagePacket;
use minecraft_packets::play::commands_packet::{
    Command, CommandArgument, CommandsPacket, StringBehavior,
};
use minecraft_packets::play::game_event_packet::GameEventPacket;
use minecraft_packets::play::legacy_chat_message_packet::LegacyChatMessagePacket;
use minecraft_packets::play::legacy_set_title_packet::LegacySetTitlePacket;
use minecraft_packets::play::login_packet::LoginPacket;
use minecraft_packets::play::player_info_update_packet::PlayerInfoUpdatePacket;
use minecraft_packets::play::scoreboard_packets::{
    SetDisplayObjectivePacket, SetObjectivePacket, SetPlayerTeamPacket, SetScorePacket,
};
use minecraft_packets::play::server_data_packet::ServerDataPacket;
use minecraft_packets::play::set_action_bar_text_packet::SetActionBarTextPacket;
use minecraft_packets::play::set_chunk_cache_center_packet::SetCenterChunkPacket;
use minecraft_packets::play::set_default_spawn_position_packet::SetDefaultSpawnPositionPacket;
use minecraft_packets::play::set_entity_data_packet::SetEntityMetadataPacket;
use minecraft_packets::play::set_subtitle_text_packet::SetSubtitleTextPacket;
use minecraft_packets::play::set_title_text_packet::SetTitleTextPacket;
use minecraft_packets::play::set_titles_animation::SetTitlesAnimationPacket;
use minecraft_packets::play::synchronize_player_position_packet::SynchronizePlayerPositionPacket;
use minecraft_packets::play::system_chat_message_packet::SystemChatMessagePacket;
use minecraft_packets::play::tab_list_packet::TabListPacket;
use minecraft_packets::play::update_time_packet::UpdateTimePacket;
use minecraft_protocol::prelude::{Dimension as ProtocolDimension, ProtocolVersion, State};
use net::raw_packet::RawPacket;
use pico_precomputed_registries::PrecomputedRegistries;
use pico_registries::Identifier;
use pico_registries::registry_provider::RegistryProvider;
use pico_registries::registry_provider::{Dimension as RegistryDimension, DimensionInfo};
use pico_structures::prelude::SchematicError;
use pico_text_component::prelude::Component;
use std::num::TryFromIntError;
use std::sync::Arc;

impl PacketHandler for AcknowledgeConfigurationPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        let mut batch = Batch::new();
        send_play_packets(&mut batch, client_state, server_state)?;
        Ok(batch)
    }
}

fn build_login_packet(
    protocol_version: ProtocolVersion,
    spawn_dimension: ProtocolDimension,
) -> Result<LoginPacket, PacketHandlerError> {
    let registry_provider = PrecomputedRegistries::new(protocol_version);
    if protocol_version.between_inclusive(ProtocolVersion::V1_7_2, ProtocolVersion::V1_15_2) {
        Ok(LoginPacket::with_dimension_pre_v1_16(spawn_dimension))
    } else if protocol_version.between_inclusive(ProtocolVersion::V1_16, ProtocolVersion::V1_16_1)
        || protocol_version.between_inclusive(ProtocolVersion::V1_19, ProtocolVersion::V1_20)
    {
        let registry_codec = registry_provider.get_registry_codec_v1_16()?;
        Ok(LoginPacket::with_registry_codec(
            spawn_dimension,
            registry_codec,
        ))
    } else if protocol_version.between_inclusive(ProtocolVersion::V1_16_2, ProtocolVersion::V1_18_2)
    {
        let registry_codec = registry_provider.get_registry_codec_v1_16()?;
        let dimension_codec = registry_provider
            .get_dimension_codec_v1_16_2(to_registry_dimension(spawn_dimension))?;
        Ok(LoginPacket::with_dimension_codec(
            spawn_dimension,
            registry_codec,
            dimension_codec,
        ))
    } else if protocol_version.between_inclusive(ProtocolVersion::V1_20_2, ProtocolVersion::V1_20_3)
    {
        Ok(LoginPacket::with_dimension_post_v1_20_2(spawn_dimension))
    } else if protocol_version.is_after_inclusive(ProtocolVersion::V1_20_5) {
        let dimension_type =
            registry_provider.get_dimension_info(to_registry_dimension(spawn_dimension))?;
        Ok(LoginPacket::with_dimension_index(
            spawn_dimension,
            i32::try_from(dimension_type.protocol_id)?,
        ))
    } else {
        Err(PacketHandlerError::invalid_state(&format!(
            "Cannot build login packet for version {protocol_version}",
        )))
    }
}

const fn to_registry_dimension(protocol_dimension: ProtocolDimension) -> RegistryDimension {
    match protocol_dimension {
        ProtocolDimension::Overworld => RegistryDimension::Overworld,
        ProtocolDimension::Nether => RegistryDimension::Nether,
        ProtocolDimension::End => RegistryDimension::End,
    }
}

const F64_CONVERSION_FAILED: &str = "Conversion failed: Invalid or out-of-range float";

fn safe_f64_to_i32(f: f64) -> Option<i32> {
    if f.is_finite() && f >= f64::from(i32::MIN) && f <= f64::from(i32::MAX) {
        #[allow(clippy::cast_possible_truncation)]
        Some(f as i32)
    } else {
        None
    }
}

fn world_position_to_chunk_position(
    position: (f64, f64),
) -> Result<(i32, i32), PacketHandlerError> {
    let chunk_x = safe_f64_to_i32((position.0 / 16.0).floor())
        .ok_or_else(|| PacketHandlerError::invalid_state(F64_CONVERSION_FAILED))?;
    let chunk_z = safe_f64_to_i32((position.1 / 16.0).floor())
        .ok_or_else(|| PacketHandlerError::invalid_state(F64_CONVERSION_FAILED))?;
    Ok((chunk_x, chunk_z))
}

type PreparedChunkPackets = ((i32, i32), Arc<[RawPacket]>);

fn prepare_chunk_packets(
    protocol_version: ProtocolVersion,
    view_distance: i32,
    dimension: ProtocolDimension,
    position: (f64, f64),
    server_state: &ServerState,
) -> Result<Option<PreparedChunkPackets>, PacketHandlerError> {
    let registry_provider = PrecomputedRegistries::new(protocol_version);
    let center_chunk = world_position_to_chunk_position(position)?;
    let biome_id = registry_provider
        .get_biome_protocol_id(&Identifier::vanilla_unchecked("plains"))
        .unwrap_or(1); // Plains biome ID is 1 before 1.13
    let dimension_info = registry_provider
        .get_dimension_info(to_registry_dimension(dimension))
        .unwrap_or_else(|_| legacy_dimension_info(dimension));

    let biome_id = i32::try_from(biome_id)?;
    let cache_key = ChunkPacketCacheKey::new(
        protocol_version,
        view_distance,
        center_chunk,
        dimension,
        biome_id,
        &dimension_info,
    );
    let world = server_state.world();
    let cache = server_state.chunk_packet_cache();
    let packets = cache
        .get_or_encode(cache_key, protocol_version, || {
            CircularChunkPacketIterator::new(
                center_chunk,
                view_distance,
                world,
                biome_id,
                &dimension_info,
                protocol_version,
            )
        })
        .map_err(PacketHandlerError::Custom)?;

    Ok(Some((center_chunk, packets)))
}

fn legacy_dimension_info(dimension: ProtocolDimension) -> DimensionInfo {
    DimensionInfo {
        height: 256,
        min_y: 0,
        protocol_id: 0,
        registry_key: match dimension {
            ProtocolDimension::Overworld => Identifier::vanilla_unchecked("overworld"),
            ProtocolDimension::Nether => Identifier::vanilla_unchecked("the_nether"),
            ProtocolDimension::End => Identifier::vanilla_unchecked("the_end"),
        },
    }
}

impl From<SchematicError> for PacketHandlerError {
    fn from(value: SchematicError) -> Self {
        Self::Custom(value.to_string())
    }
}

pub fn send_play_packets(
    batch: &mut Batch<PacketRegistry>,
    client_state: &mut ClientState,
    server_state: &ServerState,
) -> Result<(), PacketHandlerError> {
    if kick_if_lobby_full(client_state, server_state) {
        return Ok(());
    }

    let protocol_version = client_state.protocol_version();
    let view_distance = server_state.view_distance();
    let dimension = server_state.spawn_dimension();
    let reduced_debug_info = server_state.reduced_debug_info();
    let (x, y, z) = server_state.spawn_position();
    let (yaw, pitch) = server_state.spawn_rotation();
    client_state.set_position((x, y, z));
    client_state.set_rotation((yaw, pitch));

    let game_mode = {
        let expected_game_mode = server_state.game_mode();
        let is_spectator = expected_game_mode == GameMode::Spectator;

        if protocol_version.is_before_inclusive(ProtocolVersion::V1_7_6) && is_spectator {
            GameMode::Creative
        } else {
            expected_game_mode
        }
    };

    let login_packet = build_login_packet(protocol_version, dimension)?
        .set_game_mode(
            protocol_version,
            game_mode.value(),
            server_state.is_hardcore(),
        )
        .set_view_distance(view_distance)
        .set_reduced_debug_info(reduced_debug_info);

    let chunk_packets = prepare_chunk_packets(
        protocol_version,
        view_distance,
        dimension,
        (x, z),
        server_state,
    )?;

    if !register_lobby_session_for_play(client_state, server_state) {
        return Ok(());
    }

    let packet = login_packet.set_entity_id(client_state.entity_id());
    batch.push_item(PacketRegistry::Login(Box::new(packet)));

    if protocol_version.between_inclusive(ProtocolVersion::V1_19, ProtocolVersion::V1_19_1) {
        let packet = ServerDataPacket::disable_secure_profile_enforcement();
        batch.queue(|| PacketRegistry::ServerData(packet));
    }

    let is_flying = game_mode == GameMode::Spectator;
    let allow_flying = server_state.allow_flight() || is_flying;
    let packet = ClientBoundPlayerAbilitiesPacket::builder()
        .allow_flying(allow_flying)
        .creative(game_mode == GameMode::Creative)
        .flying(is_flying)
        .flying_speed(client_state.get_flying_speed())
        .build();
    batch.queue(|| PacketRegistry::ClientBoundPlayerAbilities(packet));
    client_state.set_is_flight_allowed(allow_flying);
    client_state.set_is_flying(is_flying);

    if protocol_version.is_after_inclusive(ProtocolVersion::V1_19) {
        // Send Set Default Spawn Position
        let packet = SetDefaultSpawnPositionPacket::new(dimension, x, y, z);
        batch.queue(|| PacketRegistry::SetDefaultSpawnPosition(packet));
    }

    // Send Synchronize Player Position
    let packet = SynchronizePlayerPositionPacket::new(x, y, z, yaw, pitch);
    batch.queue(|| PacketRegistry::SynchronizePlayerPosition(packet));

    if protocol_version.is_after_inclusive(ProtocolVersion::V1_13) {
        send_commands_packet(batch, protocol_version, server_state);
    }

    // The brand is not visible for clients prior to 1.13, no need to send it
    // The brand is sent during the configuration state after 1.20.2 included
    if protocol_version.between_inclusive(ProtocolVersion::V1_13, ProtocolVersion::V1_20) {
        let packet = PlayClientBoundPluginMessagePacket::brand(SERVER_BRAND);
        batch.queue(|| PacketRegistry::PlayClientBoundPluginMessage(packet));
    }

    let username = client_state.get_username();
    let placeholders = server_state.config_placeholders(&username);

    send_welcome_message(batch, server_state, protocol_version, &placeholders)?;

    let ticks = server_state.time_world_ticks();
    let lock_time = server_state.is_time_locked();
    let packet = UpdateTimePacket::new(ticks, !lock_time);
    batch.queue(|| PacketRegistry::UpdateTime(packet));

    send_scoreboard_packets(batch, client_state, server_state)?;

    send_visual_packets(
        batch,
        client_state,
        server_state,
        protocol_version,
        &placeholders,
    )?;

    if let Some((center_chunk, chunk_packets)) = chunk_packets {
        if protocol_version.is_after_inclusive(ProtocolVersion::V1_20_3) {
            // Send Game Event
            let packet = GameEventPacket::start_waiting_for_chunks(0.0);
            batch.queue(|| PacketRegistry::GameEvent(packet));
        }

        if protocol_version.is_after_inclusive(ProtocolVersion::V1_19) {
            let packet = SetCenterChunkPacket::new(center_chunk.0, center_chunk.1);
            batch.queue(|| PacketRegistry::SetCenterChunk(packet));
        }

        // Send Chunk Data and Update Light
        batch.chain_raw_packet_cache(chunk_packets);
    }

    send_selector_item_packet(batch, client_state, server_state);
    send_visibility_toggle_item_packet(batch, client_state, server_state);

    finish_play_state(client_state);

    Ok(())
}

fn send_visual_packets(
    batch: &mut Batch<PacketRegistry>,
    client_state: &ClientState,
    server_state: &ServerState,
    protocol_version: ProtocolVersion,
    placeholders: &ConfigPlaceholders<'_>,
) -> Result<(), PacketHandlerError> {
    if protocol_version.is_after_inclusive(ProtocolVersion::V1_8) {
        send_action_bar_packet(batch, server_state, protocol_version, placeholders)?;
        send_skin_packets(batch, client_state, server_state);
        send_tab_list_packets(batch, server_state, placeholders)?;
        send_title_text_packets(batch, server_state, protocol_version, placeholders)?;
    }
    if protocol_version.is_after_inclusive(ProtocolVersion::V1_9) {
        send_boss_bar_packets(batch, server_state, placeholders)?;
    }
    Ok(())
}

fn send_welcome_message(
    batch: &mut Batch<PacketRegistry>,
    server_state: &ServerState,
    protocol_version: ProtocolVersion,
    placeholders: &ConfigPlaceholders<'_>,
) -> Result<(), PacketHandlerError> {
    if let Some(component) = server_state
        .welcome_message(placeholders)
        .map_err(|err| PacketHandlerError::custom(&err.to_string()))?
    {
        send_message(batch, &component, protocol_version);
    }
    Ok(())
}

fn finish_play_state(client_state: &mut ClientState) {
    client_state.set_state(State::Play);
    client_state.set_keep_alive_should_enable();
}

fn kick_if_lobby_full(client_state: &mut ClientState, server_state: &ServerState) -> bool {
    if server_state.lobby_enabled()
        && server_state.max_players() > 0
        && server_state.online_players() >= server_state.max_players()
    {
        client_state.kick("The lobby is full.");
        true
    } else {
        false
    }
}

fn register_lobby_session_for_play(
    client_state: &mut ClientState,
    server_state: &ServerState,
) -> bool {
    if !server_state.lobby_enabled() {
        // Use entity ID 1 instead of 0 for the player, matching upstream PicoLimbo.
        // A player entity ID of 0 breaks firework elytra boosting after the client
        // is transferred to another server. Lobby mode already assigns unique
        // non-zero IDs via register_lobby_session.
        client_state.set_entity_id(1);
        return true;
    }
    if server_state.register_lobby_session(client_state).is_some() {
        true
    } else {
        client_state.kick("The lobby is full.");
        false
    }
}

fn send_scoreboard_packets(
    batch: &mut Batch<PacketRegistry>,
    client_state: &ClientState,
    server_state: &ServerState,
) -> Result<(), PacketHandlerError> {
    for packet in build_scoreboard_packets(client_state, server_state)? {
        batch.queue(move || packet);
    }
    Ok(())
}

pub fn build_scoreboard_packets(
    client_state: &ClientState,
    server_state: &ServerState,
) -> Result<Vec<PacketRegistry>, PacketHandlerError> {
    let Some(scoreboard) = server_state.scoreboard() else {
        return Ok(Vec::new());
    };
    build_scoreboard_packets_with_mode(
        client_state,
        server_state,
        scoreboard,
        ScoreboardPacketMode::Create,
    )
}

pub fn build_scoreboard_update_packets_from_rendered(
    rendered: RenderedScoreboard,
    protocol_version: ProtocolVersion,
) -> Vec<PacketRegistry> {
    build_scoreboard_packets_from_rendered(rendered, protocol_version, ScoreboardPacketMode::Update)
}

#[derive(Copy, Clone)]
enum ScoreboardPacketMode {
    Create,
    Update,
}

fn build_scoreboard_packets_with_mode(
    client_state: &ClientState,
    server_state: &ServerState,
    scoreboard: &Scoreboard,
    mode: ScoreboardPacketMode,
) -> Result<Vec<PacketRegistry>, PacketHandlerError> {
    let username = client_state.get_username();
    let placeholders = ScoreboardPlaceholders {
        player: &username,
        online: server_state.online_players(),
        max_players: server_state.max_players(),
        server: "lobby",
    };
    let rendered = scoreboard
        .render(&placeholders)
        .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
    Ok(build_scoreboard_packets_from_rendered(
        rendered,
        client_state.protocol_version(),
        mode,
    ))
}

fn build_scoreboard_packets_from_rendered(
    rendered: RenderedScoreboard,
    protocol_version: ProtocolVersion,
    mode: ScoreboardPacketMode,
) -> Vec<PacketRegistry> {
    let objective_name = Scoreboard::objective_name();
    let mut packets = Vec::with_capacity(2 + rendered.lines.len() * 2);

    let objective = match mode {
        ScoreboardPacketMode::Create => SetObjectivePacket::create(objective_name, rendered.title),
        ScoreboardPacketMode::Update => SetObjectivePacket::update(objective_name, rendered.title),
    };
    packets.push(PacketRegistry::SetObjective(objective));
    if matches!(mode, ScoreboardPacketMode::Create) {
        packets.push(PacketRegistry::SetDisplayObjective(
            SetDisplayObjectivePacket::sidebar(objective_name),
        ));
    }

    let line_count = i32::try_from(rendered.lines.len()).unwrap_or(0);
    for (index, line) in rendered.lines.into_iter().enumerate() {
        let entry = scoreboard_entry(index);
        let team_name = format!("plsb{index:02}");
        let score = line_count - i32::try_from(index).unwrap_or(0);
        let (prefix, suffix) = scoreboard_line_parts(line, protocol_version);
        let team = match mode {
            ScoreboardPacketMode::Create => SetPlayerTeamPacket::create(
                team_name,
                Component::new(""),
                prefix,
                suffix,
                vec![entry.clone()],
            ),
            ScoreboardPacketMode::Update => SetPlayerTeamPacket::update(
                team_name,
                Component::new(""),
                prefix,
                suffix,
                vec![entry.clone()],
            ),
        };
        packets.push(PacketRegistry::SetPlayerTeam(team));
        packets.push(PacketRegistry::SetScore(SetScorePacket::change(
            entry,
            objective_name,
            score,
        )));
    }

    packets
}

fn scoreboard_line_parts(
    line: Component,
    protocol_version: ProtocolVersion,
) -> (Component, Component) {
    if protocol_version.is_after_inclusive(ProtocolVersion::V1_13) {
        return (line, Component::new(""));
    }

    let legacy = line.to_legacy_text();
    let (prefix, remainder) = split_legacy_team_part(&legacy, 16);
    if remainder.is_empty() {
        return (Component::new(prefix), Component::new(""));
    }

    let formatting = active_legacy_formatting(&prefix);
    let suffix_source = format!("{formatting}{remainder}");
    let (suffix, _) = split_legacy_team_part(&suffix_source, 16);
    (Component::new(prefix), Component::new(suffix))
}

fn split_legacy_team_part(value: &str, max_chars: usize) -> (String, String) {
    let mut indices = value.char_indices();
    let Some((split_byte, last_char)) = indices.nth(max_chars.saturating_sub(1)) else {
        return (value.to_string(), String::new());
    };
    if indices.next().is_none() {
        return (value.to_string(), String::new());
    }

    let split_byte = if last_char == '\u{00a7}' {
        split_byte
    } else {
        split_byte + last_char.len_utf8()
    };

    (
        value[..split_byte].to_string(),
        value[split_byte..].to_string(),
    )
}

fn active_legacy_formatting(value: &str) -> String {
    let mut color = None;
    let mut formats = Vec::new();
    let mut chars = value.chars();

    while let Some(ch) = chars.next() {
        if ch != '\u{00a7}' {
            continue;
        }
        let Some(code) = chars.next().map(|code| code.to_ascii_lowercase()) else {
            break;
        };
        match code {
            '0'..='9' | 'a'..='f' => {
                color = Some(code);
                formats.clear();
            }
            'k'..='o' if !formats.contains(&code) => {
                formats.push(code);
            }
            'r' => {
                color = None;
                formats.clear();
            }
            _ => {}
        }
    }

    let mut formatting = String::new();
    if let Some(color) = color {
        formatting.push('\u{00a7}');
        formatting.push(color);
    }
    for code in formats {
        formatting.push('\u{00a7}');
        formatting.push(code);
    }
    if formatting.is_empty() {
        formatting.push('\u{00a7}');
        formatting.push('r');
    }
    formatting
}

fn scoreboard_entry(index: usize) -> String {
    const CODES: [&str; 15] = [
        "\u{00a7}0",
        "\u{00a7}1",
        "\u{00a7}2",
        "\u{00a7}3",
        "\u{00a7}4",
        "\u{00a7}5",
        "\u{00a7}6",
        "\u{00a7}7",
        "\u{00a7}8",
        "\u{00a7}9",
        "\u{00a7}a",
        "\u{00a7}b",
        "\u{00a7}c",
        "\u{00a7}d",
        "\u{00a7}e",
    ];
    CODES.get(index).unwrap_or(&"\u{00a7}f").to_string()
}

fn send_selector_item_packet(
    batch: &mut Batch<PacketRegistry>,
    client_state: &ClientState,
    server_state: &ServerState,
) {
    if !server_state.lobby_enabled() {
        return;
    }
    let Some(selector) = server_state.lobby_selector() else {
        return;
    };
    let version = client_state.protocol_version();
    let Some(packet) = selector.build_hotbar_packet(version) else {
        return;
    };
    batch.queue(|| PacketRegistry::SetContainerSlot(packet));
}

fn send_visibility_toggle_item_packet(
    batch: &mut Batch<PacketRegistry>,
    client_state: &ClientState,
    server_state: &ServerState,
) {
    if !server_state.lobby_enabled() {
        return;
    }
    let Some(toggle) = server_state.lobby_visibility_toggle() else {
        return;
    };
    let version = client_state.protocol_version();
    // New players always start with players visible = true.
    let Some(packet) = toggle.build_hotbar_packet(true, version) else {
        return;
    };
    batch.queue(|| PacketRegistry::SetContainerSlot(packet));
}

fn send_tab_list_packets(
    batch: &mut Batch<PacketRegistry>,
    server_state: &ServerState,
    placeholders: &ConfigPlaceholders<'_>,
) -> Result<(), PacketHandlerError> {
    if let Some(TabList { header, footer }) = server_state.tab_list() {
        let header = ServerState::render_config_component(header, placeholders)
            .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
        let footer = ServerState::render_config_component(footer, placeholders)
            .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
        let packet = TabListPacket::new(&header, &footer);
        batch.queue(|| PacketRegistry::TabList(packet));
    }
    Ok(())
}

fn send_boss_bar_packets(
    batch: &mut Batch<PacketRegistry>,
    server_state: &ServerState,
    placeholders: &ConfigPlaceholders<'_>,
) -> Result<(), PacketHandlerError> {
    if let Some(boss_bar) = server_state.boss_bar() {
        let title = ServerState::render_config_component(&boss_bar.title, placeholders)
            .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
        let packet = BossBarPacket::add(&title, boss_bar.health, boss_bar.color, boss_bar.division);
        batch.queue(|| PacketRegistry::BossBar(packet));
    }
    Ok(())
}

fn send_title_text_packets(
    batch: &mut Batch<PacketRegistry>,
    server_state: &ServerState,
    protocol_version: ProtocolVersion,
    placeholders: &ConfigPlaceholders<'_>,
) -> Result<(), PacketHandlerError> {
    if let Some(Title {
        content,
        fade_in,
        stay,
        fade_out,
    }) = server_state.title()
    {
        if protocol_version.is_after_inclusive(ProtocolVersion::V1_17) {
            let animation_packet = SetTitlesAnimationPacket::new(*fade_in, *stay, *fade_out);
            batch.queue(|| PacketRegistry::SetTitlesAnimation(animation_packet));

            match content {
                TitleType::Title(title) => {
                    let title = ServerState::render_config_component(title, placeholders)
                        .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
                    let title_packet = SetTitleTextPacket::new(&title);
                    batch.queue(|| PacketRegistry::SetTitleText(title_packet));
                }
                TitleType::Subtitle(subtitle) => {
                    let subtitle = ServerState::render_config_component(subtitle, placeholders)
                        .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
                    let subtitle_packet = SetSubtitleTextPacket::new(&subtitle);
                    batch.queue(|| PacketRegistry::SetSubtitleText(subtitle_packet));
                }
                TitleType::Both { title, subtitle } => {
                    let title = ServerState::render_config_component(title, placeholders)
                        .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
                    let subtitle = ServerState::render_config_component(subtitle, placeholders)
                        .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
                    let title_packet = SetTitleTextPacket::new(&title);
                    batch.queue(|| PacketRegistry::SetTitleText(title_packet));
                    let subtitle_packet = SetSubtitleTextPacket::new(&subtitle);
                    batch.queue(|| PacketRegistry::SetSubtitleText(subtitle_packet));
                }
            }
        } else {
            let animation_packet = LegacySetTitlePacket::set_animation(*fade_in, *stay, *fade_out);
            batch.queue(|| PacketRegistry::LegacySetTitle(animation_packet));

            match content {
                TitleType::Title(title) => {
                    let title = ServerState::render_config_component(title, placeholders)
                        .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
                    let title_packet = LegacySetTitlePacket::set_title(&title);
                    batch.queue(|| PacketRegistry::LegacySetTitle(title_packet));
                }
                TitleType::Subtitle(subtitle) => {
                    let subtitle = ServerState::render_config_component(subtitle, placeholders)
                        .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
                    let subtitle_packet = LegacySetTitlePacket::set_subtitle(&subtitle);
                    batch.queue(|| PacketRegistry::LegacySetTitle(subtitle_packet));
                }
                TitleType::Both { title, subtitle } => {
                    let title = ServerState::render_config_component(title, placeholders)
                        .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
                    let subtitle = ServerState::render_config_component(subtitle, placeholders)
                        .map_err(|err| PacketHandlerError::custom(&err.to_string()))?;
                    let title_packet = LegacySetTitlePacket::set_title(&title);
                    batch.queue(|| PacketRegistry::LegacySetTitle(title_packet));
                    let subtitle_packet = LegacySetTitlePacket::set_subtitle(&subtitle);
                    batch.queue(|| PacketRegistry::LegacySetTitle(subtitle_packet));
                }
            }
        }
    }
    Ok(())
}

fn send_action_bar_packet(
    batch: &mut Batch<PacketRegistry>,
    server_state: &ServerState,
    protocol_version: ProtocolVersion,
    placeholders: &ConfigPlaceholders<'_>,
) -> Result<(), PacketHandlerError> {
    if let Some(action_bar) = server_state
        .action_bar(placeholders)
        .map_err(|err| PacketHandlerError::custom(&err.to_string()))?
    {
        if protocol_version.is_after_inclusive(ProtocolVersion::V1_17) {
            let packet = SetActionBarTextPacket::new(&action_bar);
            batch.queue(|| PacketRegistry::SetActionBarText(packet));
        } else if protocol_version.is_after_inclusive(ProtocolVersion::V1_11) {
            let packet = LegacySetTitlePacket::action_bar(&action_bar);
            batch.queue(|| PacketRegistry::LegacySetTitle(packet));
        } else {
            let packet = LegacyChatMessagePacket::game_info(&action_bar);
            batch.queue(|| PacketRegistry::LegacyChatMessage(packet));
        }
    }
    Ok(())
}

fn send_skin_packets(
    batch: &mut Batch<PacketRegistry>,
    client_state: &ClientState,
    server_state: &ServerState,
) {
    let fetch_player_skins = server_state.fetch_player_skins();
    let is_player_listed = server_state.is_player_listed();
    let unique_id = client_state.get_unique_id();
    let protocol_version = client_state.protocol_version();

    // The skin doesn't render before 1.14, probably because there is no world?
    // However, it does render in 1.8, indicated that the packet is well implemented
    // For 1.7.x, it seems like the skin is not sent in this packet
    if protocol_version.is_after_inclusive(ProtocolVersion::V1_8) && !unique_id.is_nil() {
        let username = client_state.get_username();
        let textures = client_state.get_textures();

        batch.queue_async(move || async move {
            let textures: Option<Property> = if textures.is_some() {
                textures
            } else if fetch_player_skins {
                fetch_minecraft_profile(unique_id)
                    .await
                    .ok()
                    .and_then(|profile| profile.try_get_textures())
                    .map(|profile_property| {
                        let textures: Property = profile_property.into();
                        textures
                    })
            } else {
                None
            };

            let packet = if let Some(textures) = textures {
                PlayerInfoUpdatePacket::skin(username, unique_id, textures, is_player_listed)
            } else {
                PlayerInfoUpdatePacket::skinless(username, unique_id, is_player_listed)
            };
            PacketRegistry::PlayerInfoUpdate(packet)
        });
    }

    // There are no skin layers before 1.8 so no need to send this packet
    if protocol_version.is_after_inclusive(ProtocolVersion::V1_8) {
        let packet = SetEntityMetadataPacket::skin_layers(client_state.entity_id());
        batch.queue(|| PacketRegistry::SetEntityMetadata(packet));
    }
}

fn send_commands_packet(
    batch: &mut Batch<PacketRegistry>,
    protocol_version: ProtocolVersion,
    server_state: &ServerState,
) {
    let mut commands = vec![];
    if let ServerCommand::Enabled { alias } = server_state.server_commands().spawn() {
        commands.push(Command::no_arguments(alias));
    }
    if let ServerCommand::Enabled { alias } = server_state.server_commands().fly() {
        commands.push(Command::no_arguments(alias));
    }
    if let ServerCommand::Enabled { alias } = server_state.server_commands().fly_speed() {
        commands.push(Command::new(
            alias,
            vec![CommandArgument::float("speed", 0.0, 1.0)],
        ));
    }
    if protocol_version.is_after_inclusive(ProtocolVersion::V1_20_5)
        && let ServerCommand::Enabled { alias } = server_state.server_commands().transfer()
    {
        commands.push(Command::with_required_arguments(
            alias,
            vec![
                CommandArgument::string("hostname", StringBehavior::SingleWord),
                CommandArgument::integer("port", 0, 65535),
            ],
            1,
        ));
    }
    if let ServerCommand::Enabled { alias } = server_state.server_commands().msg() {
        commands.push(Command::with_required_arguments(
            alias,
            vec![
                CommandArgument::string("player", StringBehavior::SingleWord),
                CommandArgument::string("message", StringBehavior::GreedyPhrase),
            ],
            2,
        ));
    }
    if let ServerCommand::Enabled { alias } = server_state.server_commands().reply() {
        commands.push(Command::with_required_arguments(
            alias,
            vec![CommandArgument::string(
                "message",
                StringBehavior::GreedyPhrase,
            )],
            1,
        ));
    }
    for reply_alias in server_state.server_commands().reply_aliases() {
        if let ServerCommand::Enabled { alias } = reply_alias {
            commands.push(Command::with_required_arguments(
                alias,
                vec![CommandArgument::string(
                    "message",
                    StringBehavior::GreedyPhrase,
                )],
                1,
            ));
        }
    }
    let packet = CommandsPacket::new(commands);
    batch.queue(|| PacketRegistry::Commands(packet));
}

impl From<TryFromIntError> for PacketHandlerError {
    fn from(_: TryFromIntError) -> Self {
        Self::custom("failed to cast int")
    }
}

pub fn send_message(
    batch: &mut Batch<PacketRegistry>,
    component: &Component,
    protocol_version: ProtocolVersion,
) {
    if protocol_version.is_after_inclusive(ProtocolVersion::V1_19) {
        let packet = SystemChatMessagePacket::component(component);
        batch.queue(|| PacketRegistry::SystemChatMessage(packet));
    } else {
        let packet = LegacyChatMessagePacket::system(component);
        batch.queue(|| PacketRegistry::LegacyChatMessage(packet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::scoreboard::{ScoreboardConfig, ScoreboardEnabledMode};
    use crate::server::batch::OutboundPacket;
    use crate::server::game_profile::GameProfile;
    use futures::StreamExt;
    use minecraft_protocol::prelude::Uuid;
    use pico_text_component::prelude::parse_mini_message;

    fn server_state() -> ServerState {
        let mut builder = ServerState::builder();
        builder.view_distance(0).welcome_message("Hello, World!");
        builder.build().unwrap()
    }

    fn lobby_server_state() -> ServerState {
        let mut builder = ServerState::builder();
        builder
            .view_distance(0)
            .welcome_message("Hello, World!")
            .set_lobby_enabled(true)
            .show_online_player_count(true);
        builder.build().unwrap()
    }

    fn scoreboard_server_state(lobby_enabled: bool, mode: ScoreboardEnabledMode) -> ServerState {
        let mut builder = ServerState::builder();
        builder
            .view_distance(0)
            .welcome_message("")
            .set_lobby_enabled(lobby_enabled)
            .show_online_player_count(true)
            .scoreboard(
                ScoreboardConfig {
                    enabled: mode,
                    title: "<bold>PicoLobby</bold>".to_string(),
                    update_interval_ms: 1000,
                    lines: vec![
                        "<gray>{player}".to_string(),
                        "<green>{online}<dark_gray>/<green>{max_players}".to_string(),
                    ],
                },
                lobby_enabled,
            )
            .unwrap();
        builder.build().unwrap()
    }

    fn client(protocol: ProtocolVersion) -> ClientState {
        let mut cs = ClientState::default();
        cs.set_protocol_version(protocol);
        let previous_state = if protocol.supports_configuration_state() {
            State::Configuration
        } else {
            State::Login
        };
        cs.set_state(previous_state);
        cs
    }

    fn lobby_client(protocol: ProtocolVersion, username: &str, uuid: Uuid) -> ClientState {
        let mut cs = client(protocol);
        cs.set_game_profile(GameProfile::new(username, uuid, None));
        cs
    }

    async fn collect_play_packets(
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Vec<PacketRegistry> {
        let mut batch = Batch::new();
        send_play_packets(&mut batch, client_state, server_state).unwrap();
        batch.into_stream().collect().await
    }

    #[tokio::test]
    async fn lobby_gated_scoreboard_sends_when_lobby_is_enabled() {
        let server_state = scoreboard_server_state(true, ScoreboardEnabledMode::Lobby);
        let mut client_state = lobby_client(ProtocolVersion::V1_20_5, "Steve", Uuid::from_u128(7));

        let packets = collect_play_packets(&mut client_state, &server_state).await;

        assert!(
            packets
                .iter()
                .any(|p| matches!(p, PacketRegistry::SetObjective(_)))
        );
        assert!(
            packets
                .iter()
                .any(|p| matches!(p, PacketRegistry::SetDisplayObjective(_)))
        );
        assert_eq!(
            packets
                .iter()
                .filter(|p| matches!(p, PacketRegistry::SetPlayerTeam(_)))
                .count(),
            2
        );
        assert_eq!(
            packets
                .iter()
                .filter(|p| matches!(p, PacketRegistry::SetScore(_)))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn lobby_gated_scoreboard_omits_when_lobby_is_disabled() {
        let server_state = scoreboard_server_state(false, ScoreboardEnabledMode::Lobby);
        let mut client_state = lobby_client(ProtocolVersion::V1_20_5, "Steve", Uuid::from_u128(7));

        let packets = collect_play_packets(&mut client_state, &server_state).await;

        assert!(
            !packets
                .iter()
                .any(|p| matches!(p, PacketRegistry::SetObjective(_)))
        );
    }

    #[tokio::test]
    async fn explicitly_enabled_scoreboard_sends_without_lobby() {
        let server_state = scoreboard_server_state(false, ScoreboardEnabledMode::Always);
        let mut client_state = lobby_client(ProtocolVersion::V1_20_5, "Steve", Uuid::from_u128(7));

        let packets = collect_play_packets(&mut client_state, &server_state).await;

        assert!(
            packets
                .iter()
                .any(|p| matches!(p, PacketRegistry::SetObjective(_)))
        );
    }

    #[tokio::test]
    async fn explicitly_disabled_scoreboard_never_sends() {
        let server_state = scoreboard_server_state(true, ScoreboardEnabledMode::Never);
        let mut client_state = lobby_client(ProtocolVersion::V1_20_5, "Steve", Uuid::from_u128(7));

        let packets = collect_play_packets(&mut client_state, &server_state).await;

        assert!(
            !packets
                .iter()
                .any(|p| matches!(p, PacketRegistry::SetObjective(_)))
        );
    }

    #[test]
    fn legacy_scoreboard_line_parts_preserve_placeholder_text() {
        let line = parse_mini_message("<gray>Player: <white>Steve").unwrap();

        let (prefix, suffix) = scoreboard_line_parts(line, ProtocolVersion::V1_12_2);
        let prefix = prefix.to_legacy_text();
        let suffix = suffix.to_legacy_text();

        assert!(prefix.chars().count() <= 16);
        assert!(suffix.chars().count() <= 16);
        assert_eq!(strip_legacy_codes(&(prefix + &suffix)), "Player: Steve");
    }

    #[test]
    fn modern_scoreboard_line_parts_keep_component_in_prefix() {
        let line = parse_mini_message("<gray>Player: <white>Steve").unwrap();

        let (prefix, suffix) = scoreboard_line_parts(line.clone(), ProtocolVersion::V1_13);

        assert_eq!(prefix, line);
        assert_eq!(suffix, Component::new(""));
    }

    fn strip_legacy_codes(value: &str) -> String {
        let mut stripped = String::new();
        let mut chars = value.chars();
        while let Some(ch) = chars.next() {
            if ch == '\u{00a7}' {
                chars.next();
            } else {
                stripped.push(ch);
            }
        }
        stripped
    }

    #[tokio::test]
    async fn test_v1_20_3_play_packets() {
        // Given
        let mut client_state = client(ProtocolVersion::V1_20_3);
        let server_state = server_state();
        let mut batch = Batch::new();

        // When
        send_play_packets(&mut batch, &mut client_state, &server_state).unwrap();
        let mut batch = batch.into_outbound_stream();

        // Then
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::Login(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::ClientBoundPlayerAbilities(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SetDefaultSpawnPosition(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SynchronizePlayerPosition(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::Commands(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SystemChatMessage(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::UpdateTime(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SetEntityMetadata(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::GameEvent(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SetCenterChunk(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Raw(_)
        ));
        assert!(batch.next().await.is_none());
    }

    #[tokio::test]
    async fn test_limbo_join_assigns_entity_id_one() {
        let mut client_state = client(ProtocolVersion::V1_20_3);
        let server_state = server_state();
        let mut batch = Batch::new();

        send_play_packets(&mut batch, &mut client_state, &server_state).unwrap();
        let mut batch = batch.into_outbound_stream();

        let OutboundPacket::Registry(PacketRegistry::Login(packet)) = batch.next().await.unwrap()
        else {
            panic!("expected login packet");
        };
        assert_eq!(packet.entity_id(), 1);
        assert_eq!(client_state.entity_id(), 1);
        assert_eq!(server_state.online_players(), 0);
    }

    #[tokio::test]
    async fn test_lobby_join_assigns_unique_entity_ids_and_counts_sessions() {
        let server_state = lobby_server_state();
        let mut first = lobby_client(ProtocolVersion::V1_20_3, "First", Uuid::from_u128(1));
        let mut second = lobby_client(ProtocolVersion::V1_20_3, "Second", Uuid::from_u128(2));

        let mut first_batch = Batch::new();
        send_play_packets(&mut first_batch, &mut first, &server_state).unwrap();
        let mut second_batch = Batch::new();
        send_play_packets(&mut second_batch, &mut second, &server_state).unwrap();

        assert_eq!(first.entity_id(), 1);
        assert_eq!(second.entity_id(), 2);
        assert_eq!(server_state.online_players(), 2);

        let OutboundPacket::Registry(PacketRegistry::Login(first_login)) =
            first_batch.into_outbound_stream().next().await.unwrap()
        else {
            panic!("expected first login packet");
        };
        let OutboundPacket::Registry(PacketRegistry::Login(second_login)) =
            second_batch.into_outbound_stream().next().await.unwrap()
        else {
            panic!("expected second login packet");
        };
        assert_eq!(first_login.entity_id(), 1);
        assert_eq!(second_login.entity_id(), 2);

        assert!(
            server_state
                .unregister_lobby_session_by_entity_id(first.entity_id())
                .is_some()
        );
        assert_eq!(server_state.online_players(), 1);
    }

    #[tokio::test]
    async fn test_v1_19_play_packets() {
        // Given
        let mut client_state = client(ProtocolVersion::V1_19);
        let server_state = server_state();
        let mut batch = Batch::new();

        // When
        send_play_packets(&mut batch, &mut client_state, &server_state).unwrap();
        let mut batch = batch.into_outbound_stream();

        // Then
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::Login(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::ServerData(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::ClientBoundPlayerAbilities(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SetDefaultSpawnPosition(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SynchronizePlayerPosition(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::Commands(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::PlayClientBoundPluginMessage(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SystemChatMessage(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::UpdateTime(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SetEntityMetadata(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SetCenterChunk(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Raw(_)
        ));
        assert!(batch.next().await.is_none());
    }

    #[tokio::test]
    async fn test_v1_13_play_packets() {
        // Given
        let mut client_state = client(ProtocolVersion::V1_13);
        let server_state = server_state();
        let mut batch = Batch::new();

        // When
        send_play_packets(&mut batch, &mut client_state, &server_state).unwrap();
        let mut batch = batch.into_outbound_stream();

        // Then
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::Login(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::ClientBoundPlayerAbilities(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SynchronizePlayerPosition(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::Commands(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::PlayClientBoundPluginMessage(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::LegacyChatMessage(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::UpdateTime(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SetEntityMetadata(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Raw(_)
        ));
        assert!(batch.next().await.is_none());
    }

    #[tokio::test]
    async fn test_pre_modern_play_packets() {
        // Given
        let mut client_state = client(ProtocolVersion::V1_12_2);
        let server_state = server_state();
        let mut batch = Batch::new();

        // When
        send_play_packets(&mut batch, &mut client_state, &server_state).unwrap();
        let mut batch = batch.into_outbound_stream();

        // Then
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::Login(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::ClientBoundPlayerAbilities(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SynchronizePlayerPosition(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::LegacyChatMessage(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::UpdateTime(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Registry(PacketRegistry::SetEntityMetadata(_))
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            OutboundPacket::Raw(_)
        ));
        assert!(batch.next().await.is_none());
    }

    fn lobby_server_state_with_selector() -> ServerState {
        use crate::configuration::lobby::SelectorItemConfig;
        let mut builder = ServerState::builder();
        builder
            .view_distance(0)
            .set_lobby_enabled(true)
            .show_online_player_count(true)
            .set_lobby_selector(Some(SelectorItemConfig {
                slot: 4,
                item: "minecraft:compass".to_string(),
                display_name: Some("<bold>Selector".to_string()),
                lore: vec!["<gray>Right-click".to_string()],
                filler: None,
            }))
            .unwrap();
        builder.build().unwrap()
    }

    #[tokio::test]
    async fn selector_item_is_sent_after_chunks_when_lobby_enabled() {
        let server_state = lobby_server_state_with_selector();
        let mut client_state =
            lobby_client(ProtocolVersion::V1_20_5, "TestPlayer", Uuid::from_u128(1));
        let mut batch = Batch::new();

        send_play_packets(&mut batch, &mut client_state, &server_state).unwrap();

        let all_packets: Vec<_> = batch.into_stream().collect().await;
        let has_slot = all_packets
            .iter()
            .any(|p| matches!(p, PacketRegistry::SetContainerSlot(_)));
        assert!(has_slot, "expected SetContainerSlot in play join packets");
    }

    #[tokio::test]
    async fn selector_item_not_sent_when_no_selector_configured() {
        let server_state = lobby_server_state(); // no selector
        let mut client_state =
            lobby_client(ProtocolVersion::V1_20_5, "TestPlayer", Uuid::from_u128(1));
        let mut batch = Batch::new();

        send_play_packets(&mut batch, &mut client_state, &server_state).unwrap();

        let all_packets: Vec<_> = batch.into_stream().collect().await;
        let has_slot = all_packets
            .iter()
            .any(|p| matches!(p, PacketRegistry::SetContainerSlot(_)));
        assert!(
            !has_slot,
            "SetContainerSlot should not appear without selector config"
        );
    }
}
