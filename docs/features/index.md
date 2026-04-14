# Features

This is a quick-reference index of every feature the bot ships with: what it
does in one line, what flag turns it on, what environment variables and
`config.toml` sections it needs, and where to read more.

The bot is built so each feature is independent. There are no cross-feature
dependencies — you can run the bot with nothing but AI chat enabled, or with
nothing but Minecraft tooling, and the rest will sit quietly out of the way.

## Always-on vs opt-in

There are two flavours of feature gating in the codebase:

- **Always-on** — the code is always compiled in and the runtime always
  registers the relevant commands and event handlers. The feature only
  *activates* when its dependencies are met. AI chat, for example, is
  always-on, but if you don't set `DEEPSEEK_API_KEY` or `GEMINI_API_KEY` the
  bot just never replies to mentions. Music is similarly always-on but
  needs `yt-dlp` and `ffmpeg` on the `PATH` to actually play anything.
- **Opt-in** — the feature is gated behind a boolean in the `[features]`
  section of [`config.toml`](../configuration/instance-config.md). If the
  flag is `false` (or absent), the feature's startup hooks never run and the
  associated config sections are ignored. Auto-role, join-role, welcome, and
  the three Minecraft sub-features all work this way.

The MCP server is a special case: it is always-on but binds to a port, so
you have a separate set of `MCP_*` env vars to control where it listens and
whether it requires a bearer token.

## Feature matrix

| Feature | Config Flag | Required Env Vars | Required Config | Docs |
|---|---|---|---|---|
| AI Chat | always-on (activates if AI key set) | `DEEPSEEK_API_KEY` and/or `GEMINI_API_KEY` | `personality.txt` | [ai-chat.md](ai-chat.md) |
| Music | always-on | none | `yt-dlp` + `ffmpeg` on PATH; optional `cookies.txt` | [music.md](music.md) |
| Wordle | always-on | none | none | [games-wordle.md](games-wordle.md) |
| Connections | always-on | none | none | [games-connections.md](games-connections.md) |
| Stocks | always-on (inert without key) | `FINNHUB_API_KEY` | none | [games-stocks.md](games-stocks.md) |
| Moderation | always-on | none | none | [moderation.md](moderation.md) |
| Auto-Role Promotion | `features.auto_role = true` | none | `[auto_role]` section | [auto-role.md](auto-role.md) |
| Join Role | `features.join_role = true` | none | `[join_role]` section | [join-features.md](join-features.md) |
| Welcome Messages | `features.welcome = true` | `DEEPSEEK_API_KEY` and/or `GEMINI_API_KEY` | `[welcome]` section + `welcome_prompt.txt` | [join-features.md](join-features.md) |
| Minecraft: Verify | `features.minecraft = true`, `minecraft.verify = true` | `MC_VERIFY_URL`, `MC_VERIFY_SECRET` | `[minecraft]` section | [minecraft-verify.md](minecraft-verify.md) |
| Minecraft: Donator Sync | `features.minecraft = true`, `minecraft.donator_sync = true` | `MC_VERIFY_URL`, `MC_VERIFY_SECRET` | `[minecraft.donator_sync_config]` | [minecraft-donator-sync.md](minecraft-donator-sync.md) |
| Minecraft: Chargeback Alerts | `features.minecraft = true`, `minecraft.chargeback = true` | `MC_VERIFY_URL`, `MC_VERIFY_SECRET` | `[minecraft.chargeback_config]` | [minecraft-chargeback.md](minecraft-chargeback.md) |
| MCP Server | always-on | none (`MCP_*` optional) | none | [mcp-server.md](mcp-server.md) |

A few things worth calling out:

- The flags in the table match the field names on the `Features` struct in
  `src/instance_config.rs`. There is no `auto_role`, `join_role`, `welcome`,
  or `minecraft` flag in `[features]` if you don't put it there — they all
  default to `false`.
- When you set a `features.*` flag to `true` but forget the matching config
  section, the bot logs a warning at startup and silently skips that
  feature instead of refusing to boot. Watch the logs after editing
  `config.toml`.
- The Welcome feature is the only opt-in that needs an AI key in addition
  to its config section, because it generates greeting messages with the
  same provider stack as AI Chat.
- Stocks is "always-on" in the sense that the `!m stock` commands are
  always registered, but every command will return "Finnhub API key not
  configured" if `FINNHUB_API_KEY` is missing.

## Enabling a feature at runtime

Features are loaded once at startup. There is no live reload. To change what
is enabled:

1. Edit `config.toml` (or `.env` for env-var-driven features).
2. Restart the bot. With Docker Compose that's
   `docker compose restart <service-name>`.
3. Check the startup logs to confirm the feature was picked up — every
   opt-in feature emits an `enabled` log line on boot.

For deployment-level guidance on how to roll restarts safely, see
[Upgrading](../deployment/upgrading.md).

## Cross-references

- [Instance Config (config.toml)](../configuration/instance-config.md) —
  reference for every key in the `[features]` and per-feature sections.
- [Environment Variables](../configuration/environment-variables.md) —
  reference for every variable named in the table above.
- [Multiple Instances](../configuration/multiple-instances.md) — how to run
  several bot instances with different feature sets out of the same image.
- [Architecture overview](../architecture/index.md) — for understanding how
  features compose at runtime.
