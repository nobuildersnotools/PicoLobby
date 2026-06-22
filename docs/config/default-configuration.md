# Default Configuration

The default configuration file will be automatically generated the first time you start the server.
If it is not generated, you can copy the following code block in your configuration file or in `server.toml` next to PicoLimbo's executable.

:::code-group
```toml [server.toml]
# Server bind address and port
bind = "0.0.0.0:25565"
# Welcome message sent to players after spawning
welcome_message = "Welcome to PicoLimbo!"
action_bar = "Welcome to PicoLimbo!"
# Sets the game mode for new players
# Allowed values: "survival", "creative", "adventure", or "spectator"
default_game_mode = "spectator"
# If set to true, will spawn the player in hardcode mode
hardcore = false
# Set to true to fetch the skin textures from Mojang API
fetch_player_skins = false
reduced_debug_info = false
allow_unsupported_versions = false
allow_flight = false
accept_transfers = false

[forwarding]
# Disable forwarding
method = "NONE"

[world]
# Custom spawn position as [x, y, z] coordinates
spawn_position = [0.0, 320.0, 0.0]
# Custom spawn rotation as [yaw, pitch] coordinates
spawn_rotation = [0.0, 0.0]
# Default spawn dimension
# Allowed values: "overworld", "nether", or "end"
dimension = "end"
# Sets the time in the world
# Allowed values: "day", "noon", "night", "midnight", or a specific time in ticks (0-24000)
time = "day"

[world.experimental]
# Configure how many chunks are sent to clients
view_distance = 2
# Path to schematic file for custom world structures
# Leave empty to disable schematic loading
schematic_file = ""
# Lock the time in the world to `world.time` value
lock_time = false

[world.boundaries]
# Enable world boundaries
enabled = true
# Minimum Y position, players below this will be teleported back to spawn
min_y = -64
# Message displayed when a player reaches the minimum Y position
teleport_message = "<red>You have reached the bottom of the world.</red>"

[server_list]
reply_to_status = true
# Maximum count shown in your server list.
# When lobby mode is enabled, this also caps lobby players. Set to 0 to disable the limit.
max_players = 20
# MOTD displayed in server lists
message_of_the_day = "A Minecraft Server"
# Show actual online player count in your server list?
show_online_player_count = true
server_icon = "server-icon.png"

[lobby]
enabled = false
chat_format = "<white>&lt;{sender}&gt; {message}</white>"
join_message = "<yellow>{player} joined the game</yellow>"
leave_message = "<yellow>{player} left the game</yellow>"

[lobby.private_messages]
sender_format = "<gray>[me -> {recipient}]</gray> <white>{message}</white>"
recipient_format = "<gray>[{sender} -> me]</gray> <white>{message}</white>"
unknown_target = "<red>Player '{target}' is not online in the lobby.</red>"
ambiguous_target = "<red>More than one online player matches '{target}'.</red>"
hidden_target = "<red>{target} cannot receive private messages with hidden chat.</red>"
missing_reply_target = "<red>You have nobody to reply to.</red>"
self_message = "<red>You cannot send a private message to yourself.</red>"
empty_message = "<red>Private message cannot be empty.</red>"
too_long = "<red>Private message is too long.</red>"
rate_limit = "<red>You are sending messages too quickly.</red>"
unavailable = "<red>Private messages are only available in the lobby.</red>"

[[lobby.servers]]
id = "survival"
display_name = "Survival"
server = "survival"
# Optional per-entry overrides for how this server appears in the selector GUI:
# item = "minecraft:grass_block"            # item shown in the menu (default: minecraft:paper)
# lore = ["<gray>Click to connect."]        # MiniMessage hover lore
# slot = 0                                   # explicit GUI slot 0–26 (default: auto-placed)
# enchanted = true                           # render the item with the enchantment glint

[lobby.selector]
slot = 4
item = "minecraft:compass"
display_name = "<bold><gold>Server Selector"
lore = ["<gray>Right-click to choose a server."]

[[lobby.npcs]]
id = "survival-npc"
destination = "survival"
name = "Survival"
x = 0.0
y = 320.0
z = 4.0
yaw = 180.0
pitch = 0.0
# Delay before hiding this NPC from older-client tab lists after spawn.
# Set to 0 to keep this NPC listed for the whole session on those clients.
tab_list_remove_delay_ms = 3000
# Optional skin for this NPC. Remove the line for the default Steve/Alex skin.
# Skins render on Minecraft 1.8+ clients only; if a skin fails to resolve the
# NPC spawns skinless without blocking startup.
# Mirror an existing account by name or UUID (resolved from Mojang at startup):
skin = { player = "Notch" }
# Or provide a raw signed textures property (offline; signature is optional):
# skin = { value = "ewogICJ0aW1lc3RhbXAiIDog...", signature = "GnG2..." }

[compression]
threshold = -1
level = 6

[tab_list]
# Enable tab list customization
enabled = true
# The header text displayed at the top of the player list
header = "<bold>Welcome to PicoLimbo</bold>"
# The footer text displayed at the bottom of the player list
footer = "<green>Enjoy your stay!</green>"
player_listed = true

[boss_bar]
# Enable boss bar display
enabled = false
# Boss bar title displayed to players
title = "<bold>Welcome to PicoLimbo!</bold>"
# Boss bar health (0.0 to 1.0, where 1.0 is full health)
health = 1.0
# Boss bar color
# Allowed values: "blue", "green", "pink", "purple", "red", "white", or "yellow"
color = "pink"
# Boss bar style
# Allowed values: 0, 6, 10, 12 or 20, representing the number of segments
division = 0

[title]
enabled = false
title = "<bold>Welcome!</bold>"
subtitle = "Enjoy your stay"
fade_in = 10
stay = 70
fade_out = 20

[scoreboard]
enabled = "lobby"
title = "<bold>PicoLobby</bold>"
update_interval_ms = 1000
lines = ["<gray>Player: <white>{player}", "<gray>Online: <green>{online}<dark_gray>/<green>{max_players}", "<gray>Server: <aqua>{server}"]

[commands]
spawn = "spawn"
fly = "fly"
fly_speed = "flyspeed"
transfer = "transfer"
server = "server"
msg = "msg"
reply = "reply"
reply_aliases = ["r"]
```
:::
