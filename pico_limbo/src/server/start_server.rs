use crate::configuration::TaggedForwarding;
use crate::configuration::antispam::AntispamConfig;
use crate::configuration::boss_bar::BossBarConfig;
use crate::configuration::config::{Config, ConfigError, load_or_create};
use crate::configuration::lobby::LobbyConfig;
use crate::configuration::tab_list::TabListMode;
use crate::configuration::title::TitleConfig;
use crate::configuration::world_config::boundaries::BoundariesConfig;
use crate::server::network::Server;
use crate::server_state::{LobbyDestination, ServerState, ServerStateBuilderError};
use std::path::PathBuf;
use std::process::ExitCode;
use tracing::{Level, debug, error};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub async fn start_server(config_path: PathBuf, logging_level: u8) -> ExitCode {
    enable_logging(logging_level);
    let Some(cfg) = load_configuration(&config_path) else {
        return ExitCode::FAILURE;
    };

    let bind = cfg.bind.clone();

    match build_state(cfg) {
        Ok(server_state) => {
            Server::new(&bind, server_state).run().await;
            ExitCode::SUCCESS
        }
        Err(err) => {
            error!("Failed to start PicoLimbo: {err}");
            ExitCode::SUCCESS
        }
    }
}

fn load_configuration(config_path: &PathBuf) -> Option<Config> {
    let cfg = load_or_create(config_path);
    match cfg {
        Err(ConfigError::TomlDeserialize(message, ..)) => {
            error!("Failed to load configuration: {}", message);
        }
        Err(ConfigError::Io(message, ..)) => {
            error!("Failed to load configuration: {}", message);
        }
        Err(ConfigError::EnvPlaceholder(var)) => {
            error!("Failed to load configuration: {}", var);
        }
        Err(ConfigError::TomlSerialize(message, ..)) => {
            error!("Failed to save default configuration file: {}", message);
        }
        Ok(cfg) => return Some(cfg),
    }
    None
}

fn build_state(cfg: Config) -> Result<ServerState, ServerStateBuilderError> {
    let mut server_state_builder = ServerState::builder();
    let lobby_enabled = cfg.lobby.enabled;

    apply_world_and_visual_options(&mut server_state_builder, &cfg, lobby_enabled)?;
    apply_core_options(&mut server_state_builder, &cfg)?;
    apply_forwarding(&mut server_state_builder, cfg.forwarding.into());

    if lobby_enabled {
        apply_lobby_options(&mut server_state_builder, cfg.lobby)?;
    } else {
        server_state_builder
            .set_lobby_enabled(false)
            .antispam(AntispamConfig::disabled());
    }

    server_state_builder.server_commands(cfg.commands);
    server_state_builder.build()
}

fn apply_forwarding(
    server_state_builder: &mut crate::server_state::ServerStateBuilder,
    forwarding: TaggedForwarding,
) {
    match forwarding {
        TaggedForwarding::None => {
            server_state_builder.disable_forwarding();
        }
        TaggedForwarding::Legacy => {
            debug!("Enabling legacy forwarding");
            server_state_builder.enable_legacy_forwarding();
        }
        TaggedForwarding::BungeeGuard { tokens } => {
            server_state_builder.enable_bungee_guard_forwarding(tokens);
        }
        TaggedForwarding::Modern { secret } => {
            debug!("Enabling modern forwarding");
            server_state_builder.enable_modern_forwarding(secret);
        }
    }
}

fn apply_world_and_visual_options(
    server_state_builder: &mut crate::server_state::ServerStateBuilder,
    cfg: &Config,
    lobby_enabled: bool,
) -> Result<(), ServerStateBuilderError> {
    if let BoundariesConfig::Enabled(ref boundaries) = cfg.world.boundaries {
        if cfg.world.spawn_position.1 < f64::from(boundaries.min_y) {
            return Err(ServerStateBuilderError::InvalidSpawnPosition);
        }
        server_state_builder.boundaries(boundaries.min_y, &boundaries.teleport_message)?;
    }

    if let TabListMode::Enabled(ref tab_list) = cfg.tab_list.mode {
        server_state_builder.tab_list(&tab_list.header, &tab_list.footer)?;
    }

    if let BossBarConfig::Enabled(ref boss_bar) = cfg.boss_bar {
        server_state_builder.boss_bar(boss_bar.clone())?;
    }

    if let TitleConfig::Enabled(ref title) = cfg.title {
        server_state_builder.title(
            &title.title,
            &title.subtitle,
            title.fade_in,
            title.stay,
            title.fade_out,
        )?;
    }

    server_state_builder.scoreboard(cfg.scoreboard.clone(), lobby_enabled)?;

    let server_icon = &cfg.server_list.server_icon;
    if std::fs::exists(server_icon)? {
        server_state_builder.fav_icon(server_icon)?;
    }
    Ok(())
}

fn apply_lobby_options(
    server_state_builder: &mut crate::server_state::ServerStateBuilder,
    lobby: LobbyConfig,
) -> Result<(), ServerStateBuilderError> {
    let lobby_destinations = lobby
        .servers
        .into_iter()
        .map(|e| LobbyDestination::new(e.id, e.display_name, e.server))
        .collect::<Vec<_>>();

    server_state_builder
        .set_lobby_enabled(true)
        .antispam(lobby.antispam)
        .set_lobby_chat_format(lobby.chat_format)
        .set_lobby_private_messages(lobby.private_messages)
        .set_lobby_join_message(lobby.join_message)
        .set_lobby_leave_message(lobby.leave_message)
        .set_lobby_destinations(lobby_destinations)?
        .set_lobby_npcs(lobby.npcs)?
        .set_lobby_selector(lobby.selector)?
        .set_lobby_visibility_toggle(lobby.visibility_toggle)?;
    Ok(())
}

fn apply_core_options(
    server_state_builder: &mut crate::server_state::ServerStateBuilder,
    cfg: &Config,
) -> Result<(), ServerStateBuilderError> {
    server_state_builder
        .dimension(cfg.world.dimension.clone().into())
        .time_world(cfg.world.time.clone().into())
        .lock_time(cfg.world.experimental.lock_time)
        .description_text(&cfg.server_list.message_of_the_day)
        .welcome_message(&cfg.welcome_message)
        .action_bar(&cfg.action_bar)?
        .max_players(cfg.server_list.max_players)
        .show_online_player_count(cfg.server_list.show_online_player_count)
        .game_mode(cfg.default_game_mode.clone().into())
        .hardcore(cfg.hardcore)
        .spawn_position(cfg.world.spawn_position)
        .spawn_rotation(cfg.world.spawn_rotation)
        .view_distance(cfg.world.experimental.view_distance)
        .schematic(cfg.world.experimental.schematic_file.clone())
        .enable_compression(cfg.compression.threshold, cfg.compression.level)?
        .fetch_player_skins(cfg.fetch_player_skins)
        .reduced_debug_info(cfg.reduced_debug_info)
        .set_player_listed(cfg.tab_list.player_listed)
        .set_reply_to_status(cfg.server_list.reply_to_status)
        .set_allow_unsupported_versions(cfg.allow_unsupported_versions)
        .set_allow_flight(cfg.allow_flight)
        .set_accept_transfers(cfg.accept_transfers);
    Ok(())
}

fn enable_logging(verbose: u8) {
    let log_level = match verbose {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(log_level.into()))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_lobby_does_not_validate_lobby_destinations() {
        let mut cfg = Config::default();
        cfg.lobby.enabled = false;
        cfg.lobby.servers[0].server.clear();

        assert!(build_state(cfg).is_ok());
    }

    #[test]
    fn enabled_lobby_validates_lobby_destinations() {
        let mut cfg = Config::default();
        cfg.lobby.enabled = true;
        cfg.lobby.servers[0].server.clear();

        assert!(matches!(
            build_state(cfg),
            Err(ServerStateBuilderError::EmptyServerName(id)) if id == "survival"
        ));
    }
}
