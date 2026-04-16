# Instance Config (`config.toml`)

`config.toml` describes a bot instance's identity, command surface, and feature configuration. Every field on this page comes from the `InstanceConfig`, `Features`, `AutoRoleConfig`, `JoinRoleConfig`, `WelcomeConfig`, `MinecraftConfig`, `DonatorSyncConfig`, and `ChargebackConfig` structs in [`src/instance_config.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/instance_config.rs). If you find a field in the source that isn't documented here, please open an issue.

The file is read once at startup from `CONFIG_DIR/config.toml` (where `CONFIG_DIR` defaults to the working directory and is `/config` inside the Docker image). Parse failures panic with a path and the underlying TOML error — you'll see them immediately when the container starts.

## Top-level fields

| Field              | Type   | Required | Default            | Description                                            |
|--------------------|--------|----------|--------------------|--------------------------------------------------------|
| `bot_name`         | string | yes      | —                  | Display name shown in help output and logs             |
| `command_prefix`   | string | yes      | —                  | Prefix for text-based commands                         |
| `personality_file` | string | no       | `"personality.txt"`| Path to the personality file, relative to `config.toml`|

### `bot_name`

The human-readable name of this instance. It is used in help output, in welcome messages where templates reference it, and in startup log lines so you can tell which instance you're looking at when you tail several at once.

### `command_prefix`

The prefix the bot listens for on text commands. The repo ships with `"!"` in the example, but you can use anything that won't collide with normal chat. discord-bot-rs uses prefix commands only — there are no slash commands — so this setting controls how users invoke every command.

### `personality_file`

The filename (relative to the same directory as `config.toml`) where the AI system prompt lives. Defaults to `personality.txt`. The file must exist and must not be empty when AI chat is active — the loader panics if either condition fails. See [Personality Files](personality.md) for how to write one.

## `[features]` section

The `features` table holds the master feature flags. Every flag defaults to `false`, so an empty `[features]` section (or no section at all) means a stripped-down bot that only does AI chat, music, games, and the always-on commands.

| Field       | Type | Default | Gates                                                                |
|-------------|------|---------|----------------------------------------------------------------------|
| `minecraft` | bool | `false` | The Minecraft module (`verify`, `donator_sync`, `chargeback`)        |
| `auto_role` | bool | `false` | Periodic auto-role promotion based on member age and activity        |
| `join_role` | bool | `false` | Assigning a role to every new member on join                         |
| `welcome`   | bool | `false` | AI-generated welcome messages on member join                         |

Setting a flag to `false` disables the entire feature area; the corresponding sub-section (`[auto_role]`, `[minecraft]`, etc.) is not required and will be ignored if present. Setting a flag to `true` activates the loader for the matching sub-section. If the sub-section is missing the bot logs a warning at startup and disables the feature anyway, rather than panicking — see [Validation](#validation) below.

## `[auto_role]` section

Required when `features.auto_role = true`. The auto-role module periodically scans members who currently have `from_role` and grants them `to_role` (and removes `from_role`) once they meet the configured criteria. The first scan runs at bot startup; subsequent scans run on a fixed schedule.

| Field          | Type   | Required | Default | Description                                              |
|----------------|--------|----------|---------|----------------------------------------------------------|
| `from_role`    | string | yes      | —       | Snowflake of the role to remove                          |
| `to_role`      | string | yes      | —       | Snowflake of the role to grant                           |
| `min_age`      | string | no       | `"3d"`  | Minimum time in the guild before promotion               |
| `min_messages` | int    | no       | `20`    | Minimum messages sent in the guild before promotion      |
| `require_all`  | bool   | no       | `false` | If `true`, both criteria must hold; if `false`, either   |

### Duration format for `min_age`

`min_age` is a short duration string parsed by `parse_duration` in [`src/util/duration.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/util/duration.rs). The grammar is `<integer><unit>` with no spaces, where unit is one of:

| Suffix | Unit    | Example                |
|--------|---------|------------------------|
| `s`    | seconds | `30s`                  |
| `m`    | minutes | `15m`                  |
| `h`    | hours   | `2h`                   |
| `d`    | days    | `3d` (the default)     |
| `w`    | weeks   | `1w`                   |

Combined units (`1d12h`) are not supported — pick the largest unit that expresses what you want. The maximum allowed duration is 365 days; anything longer is rejected.

### How the criteria combine

With `require_all = false` (the default), a member is promoted as soon as either condition holds: they've been in the guild long enough, or they've sent enough messages. With `require_all = true`, both must hold. Newly added IDs and recent message counts are picked up between scans, so the lag between meeting the threshold and getting promoted is bounded by the scan interval.

See [Auto-Role](../features/auto-role.md) for behavioral details.

## `[join_role]` section

Required when `features.join_role = true`. Assigns a single role to every new member on join. Most often used to mark unverified accounts that haven't gone through whatever onboarding flow your server has set up.

| Field  | Type   | Required | Default | Description                          |
|--------|--------|----------|---------|--------------------------------------|
| `role` | string | yes      | —       | Snowflake of the role to grant       |

See [Join Features](../features/join-features.md).

## `[welcome]` section

Required when `features.welcome = true`. Posts an AI-generated welcome message to a designated channel when a new member joins. The bot needs at least one AI provider key (`DEEPSEEK_API_KEY` or `GEMINI_API_KEY`); without one, it logs a warning at startup and the feature stays inactive.

| Field         | Type   | Required | Default                | Description                                  |
|---------------|--------|----------|------------------------|----------------------------------------------|
| `channel`     | string | yes      | —                      | Snowflake of the channel to post into        |
| `prompt_file` | string | no       | `"welcome_prompt.txt"` | File (relative to `config.toml`) holding the welcome prompt template |

The prompt file is loaded by `load_welcome_prompt` in `src/instance_config.rs`; if it's missing or empty the bot logs a warning and welcome stays off. See [Join Features](../features/join-features.md).

## `[minecraft]` section

Required when `features.minecraft = true`. The Minecraft module is itself a bundle of three independently toggleable sub-features. All three depend on the bot being able to talk to a companion plugin on the Minecraft server, which means `MC_VERIFY_URL` and `MC_VERIFY_SECRET` must be set in the instance's `.env`.

| Field           | Type   | Required | Default | Description                                              |
|-----------------|--------|----------|---------|----------------------------------------------------------|
| `verify`        | bool   | no       | `true`  | Enable the `!m verify` linking command                   |
| `donator_sync`  | bool   | no       | `false` | Poll the MC server and sync donator-tier roles           |
| `chargeback`    | bool   | no       | `false` | Receive chargeback alerts from the MC store              |

`verify` defaults to `true` so that the most common case (you want player-account linking) needs no extra configuration beyond enabling the module. `donator_sync` and `chargeback` default to `false` and need their own sub-sections (below) before they do anything useful.

See [Minecraft Verify](../features/minecraft-verify.md), [Minecraft Donator Sync](../features/minecraft-donator-sync.md), and [Minecraft Chargeback](../features/minecraft-chargeback.md).

## `[minecraft.donator_sync_config]` section

Required when `minecraft.donator_sync = true`. Polls the Minecraft server on an interval and synchronizes Discord roles to match each user's purchased tier.

| Field              | Type   | Required | Default | Description                                              |
|--------------------|--------|----------|---------|----------------------------------------------------------|
| `supporter_role`   | string | yes      | —       | Snowflake of the supporter-tier role                     |
| `premium_role`     | string | yes      | —       | Snowflake of the premium-tier role                       |
| `check_interval`   | int    | no       | `300`   | Seconds between donator-tier checks                      |

The default 300-second interval is conservative enough to avoid spamming the MC server while keeping role state reasonably fresh. See [Minecraft Donator Sync](../features/minecraft-donator-sync.md).

## `[minecraft.chargeback_config]` section

Required when `minecraft.chargeback = true`. The bot exposes an HTTP endpoint that receives chargeback notifications from the MC store; on receipt it strips the user's roles, applies a restricted role, and posts an interactive alert to a staff channel with Ban and Dismiss buttons.

| Field              | Type           | Required | Default | Description                                  |
|--------------------|----------------|----------|---------|----------------------------------------------|
| `staff_channel`    | string         | yes      | —       | Snowflake of the channel to post alerts in   |
| `restricted_role`  | string         | yes      | —       | Snowflake of the role to apply to offenders  |
| `staff_roles`      | list of string | no       | `[]`    | Snowflakes allowed to press Ban/Dismiss buttons. Empty list means no role can act on alerts (the buttons reply "You don't have permission to do this." for everyone), so configure this if you want staff to be able to confirm or dismiss chargebacks. |

See [Minecraft Chargeback](../features/minecraft-chargeback.md).

## Validation

At startup the loader inspects each enabled feature flag and tries to find its sub-section:

- **TOML parse errors** panic with the file path and the parser's error message. Fix the syntax and restart.
- **Missing `bot_name` or `command_prefix`** is a TOML parse error (they're required fields with no default), so they fall into the same case.
- **Missing personality file** when AI chat is implicitly active panics with the file path. An empty file panics with a different message asking you to fill it in.
- **`features.minecraft = true` with no `[minecraft]` section** logs a warning and disables the Minecraft module. The bot still starts.
- **`features.auto_role = true` with no `[auto_role]` section** logs a warning and disables auto-role. The bot still starts.
- **`features.join_role = true` with no `[join_role]` section** logs a warning and disables join-role. The bot still starts.
- **`features.welcome = true` with no `[welcome]` section, no prompt file, or an empty prompt file** logs a warning and disables welcomes. The bot still starts.
- **`minecraft.donator_sync = true` with no `[minecraft.donator_sync_config]`** logs a warning and disables donator sync.
- **`minecraft.chargeback = true` with no `[minecraft.chargeback_config]`** logs a warning and disables chargeback.

Validation strategy is "loud warnings, soft fail": misconfiguration of optional features doesn't crash the bot, but it does show up clearly in the logs. Run with `RUST_LOG=info,discord_bot=info` (or similar) to see the per-module enable/disable lines on startup.

## Complete annotated example

The example instance ships with this `config.toml` (also at `instances/example/config.toml` in the repo):

```toml
# ============================================================================
# discord-bot-rs — Example Instance Configuration
# ============================================================================
#
# This file is the authoritative reference for every config.toml option.
# Copy this whole directory to create a new instance:
#
#     cp -r instances/example instances/mybot
#
# Then edit this file, instances/mybot/.env, and instances/mybot/personality.txt.
# ============================================================================


# ----- Identity -------------------------------------------------------------

# Display name shown in bot output (help text, welcome messages, etc.)
bot_name = "Example Bot"

# Prefix for all user commands. discord-bot-rs uses prefix commands only
# (no slash commands), so this is the single prefix every user sees.
command_prefix = "!"

# Path to the personality file (system prompt for AI chat), relative to this
# config file. Default: "personality.txt"
personality_file = "personality.txt"


# ----- Feature Flags --------------------------------------------------------
#
# Each flag gates a whole feature area. Setting to false disables the feature
# entirely — the corresponding config section below is not required.

[features]
minecraft = false     # Minecraft verification + donator sync + chargeback alerts
auto_role = false     # Automatic role promotion based on activity
join_role = false     # Assign a role to new members on join
welcome = false       # AI-generated welcome message on join


# ----- Auto-Role Promotion --------------------------------------------------
#
# Required when features.auto_role = true.
# Promotes members from `from_role` to `to_role` once they meet activity
# criteria. Runs periodically; the first pass happens at bot startup.

# [auto_role]
# from_role = "ROLE_ID"       # Snowflake of the role to remove
# to_role = "ROLE_ID"         # Snowflake of the role to grant
# min_age = "3d"              # Time in guild before eligible (e.g. "1h", "3d", "1w")
# min_messages = 20           # Messages sent in the guild before eligible
# require_all = false         # true = both criteria required, false = either


# ----- Join Role ------------------------------------------------------------
#
# Required when features.join_role = true.

# [join_role]
# role = "ROLE_ID"            # Snowflake of the role to grant on join


# ----- Welcome Messages -----------------------------------------------------
#
# Required when features.welcome = true.

# [welcome]
# channel = "CHANNEL_ID"
# prompt_file = "welcome_prompt.txt"   # Relative to this config file


# ----- Minecraft Module -----------------------------------------------------
#
# Required when features.minecraft = true.
# Any sub-feature can be independently toggled.
# MC_VERIFY_URL and MC_VERIFY_SECRET in .env are required for all sub-features.

# [minecraft]
# verify = true               # !m verify command (default: true)
# donator_sync = false        # Poll MC server for donator tier role sync
# chargeback = false          # Webhook listener for chargeback alerts


# ----- Donator Sync ---------------------------------------------------------
#
# Required when minecraft.donator_sync = true.

# [minecraft.donator_sync_config]
# supporter_role = "ROLE_ID"
# premium_role = "ROLE_ID"
# check_interval = 300        # Poll interval in seconds (default: 300)


# ----- Chargeback Alerts ----------------------------------------------------
#
# Required when minecraft.chargeback = true.

# [minecraft.chargeback_config]
# staff_channel = "CHANNEL_ID"
# restricted_role = "ROLE_ID"
# staff_roles = ["MOD_ROLE_ID", "ADMIN_ROLE_ID", "OWNER_ROLE_ID"]   # Snowflakes allowed to press Ban/Dismiss; empty = no one
```

To turn any feature on, flip its flag in `[features]` and uncomment the matching sub-section, replacing `ROLE_ID` and `CHANNEL_ID` placeholders with real Discord snowflakes (right-click → *Copy ID* with Developer Mode enabled).
