# Scoreboard

The scoreboard config controls the sidebar shown to players after they enter the play state.

```toml
[scoreboard]
enabled = "lobby"
title = "<bold>PicoLobby</bold>"
update_interval_ms = 1000
lines = [
  "<gray>Player: <white>{player}",
  "<gray>Online: <green>{online}<dark_gray>/<green>{max_players}",
  "<gray>Server: <aqua>{server}",
]
```

`enabled` accepts:

- `"lobby"`: show the scoreboard only when `[lobby].enabled = true`.
- `true`: always show the scoreboard.
- `false`: never show the scoreboard.

The first scoreboard implementation is sidebar-only. Up to 15 lines are supported, matching the practical sidebar limit. Duplicate-looking lines are supported because PicoLobby sends hidden unique entry keys for each row.

Scoreboard text supports the same runtime placeholders as other user-facing text templates: `{player}`, `{online}`, `{max_players}`, and `{server}`. Unknown placeholders are left unchanged.
