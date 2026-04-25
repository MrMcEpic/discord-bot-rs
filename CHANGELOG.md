# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- (track new changes here until the next release)

## [0.18.0] - 2026-04-25

### Changed
- **Default DeepSeek registry now uses V4 model identifiers.** `deepseek_chat`
  defaults to `deepseek-v4-flash` (was `deepseek-chat`). `deepseek_reasoner`
  defaults to `deepseek-v4-pro` (was `deepseek-reasoner`) — the V4 flagship.
  The reasoner's per-provider `max_tokens` cap is bumped from 32768 to 65536
  to accommodate V4-Pro's larger thinking budget; the chat cap stays at 8192
  (Discord-bound). Motivated by DeepSeek's announced retirement of the legacy
  `deepseek-chat` and `deepseek-reasoner` model strings on 2026-07-24.
- **Heads-up on V4-Pro pricing.** V4-Pro output costs roughly 12× V4-Flash per
  token. Instances that don't want flagship pricing on the reasoner role can
  route the reasoner role at V4-Flash with one line in `[ai.routing]`:
  ```toml
  [ai.routing]
  reasoner = "deepseek_chat"
  ```
  Or disable the reasoner role entirely by setting `[ai.routing]` and
  omitting `reasoner`. See the new "Disabling V4-Pro flagship" section in
  [`docs/configuration/ai-providers.md`](docs/configuration/ai-providers.md)
  for both patterns.

### Added
- **Four new aliases preserved by `ProviderRouter::named()`:** `deepseek-v4`
  and `deepseek-v4-flash` resolve to `deepseek_chat`; `deepseek-v4-pro` and
  `deepseek-reasoner` resolve to `deepseek_reasoner`. The `deepseek-reasoner`
  alias keeps `[ai.fallback] on_censored` configs working past 2026-07-24.
  All 0.14.0 aliases (`gemini`, `deepseek`, `deepseek-chat`) remain.

### Tests
- 4 new alias-resolution assertions in
  `router_named_resolves_v0_14_0_short_aliases_for_backward_compat` covering
  every new V4 alias and the deprecated `deepseek-reasoner` spelling.
- Snapshot tests `default_registry_deepseek_chat_matches_*` and
  `default_registry_deepseek_reasoner_matches_*` renamed `_v0_14_0` →
  `_v0_18_0` and updated for the new model strings + 65536 cap.

## [0.17.0] - 2026-04-17

### Added
- **`timezone` field on `InstanceConfig`** — optional IANA zone name
  (`"America/Toronto"`, `"Europe/London"`, …) used to format the
  "current date / time" line in the AI system prompt. When set, the prompt
  states the local date, local clock time, IANA zone name, and numeric UTC
  offset in one line. When unset, the prompt falls back to UTC and labels
  it explicitly. See
  [`docs/configuration/instance-config.md`](docs/configuration/instance-config.md)
  for details.

### Fixed
- **AI system prompt no longer reports tomorrow's date late at night.**
  Previously the system prompt embedded `chrono::Utc::now()` formatted as a
  bare date with no timezone, so a user chatting at 10:52 PM Eastern saw
  the model confidently announce "Today is Saturday, April 18" (UTC had
  rolled over; their local day hadn't). The model also regularly
  hallucinated plausible-sounding cities to justify the date. The prompt
  now always carries a timezone label — either the configured IANA zone
  or an explicit "UTC" callout — and an anti-drift instruction telling
  the model not to second-guess the stated time.
- **Typos in the timezone name fail loudly at startup** with a message
  that includes the offending string, instead of silently defaulting to
  UTC. Docs steer users toward full IANA names (e.g. `America/New_York`)
  rather than bare abbreviations (e.g. `EST`) so `chrono-tz` handles
  daylight saving correctly.

### Dependencies
- Added `chrono-tz = "0.10"` for IANA timezone resolution.

### Tests
- 4 new tests in `instance_config.rs` covering timezone parsing (accepts
  common IANA names, rejects gibberish with a helpful message,
  default-to-`None` behaviour, resolve-to-`Tz` when set).
- 3 new tests in `ai/chat.rs` covering the prompt's UTC fallback wording,
  the presence of local time / zone name / numeric offset when a timezone
  is configured, and the anti-drift instruction.

## [0.16.0] - 2026-04-17

### Added
- **Native Anthropic-spec dispatcher** (#29, closes phase 2 of #28). New `spec = "anthropic"` value on provider definitions routes to `POST /v1/messages` via a dedicated `complete_anthropic` translation layer instead of the OpenAI `complete` path. Translation is bidirectional: OpenAI-shape messages / tool definitions / tool results get transformed to Anthropic wire shape on input, and Claude's `content` blocks / `tool_use` blocks get flattened back into the uniform `ApiResponse` shape on output. `chat.rs` stays provider-agnostic — adding a new spec is ~15 new translation functions plus one match arm in `complete_dispatch`.
- **Claude example in `defaults/example-providers.toml`** — worked commented block showing the three Anthropic-specific fields (`auth_header = "x-api-key"`, `auth_scheme = ""`, `headers = { "anthropic-version" = "2023-06-01" }`) and `claude-opus-4-7` as the default model. Claude is NOT in the baked default registry; users opt in explicitly.
- **Three new optional `ProviderDef` fields** for configurable auth + extra headers (usable by both OpenAI and Anthropic providers):
  - `headers: HashMap<String, String>` (default empty) — extra request headers
  - `auth_header: String` (default `"Authorization"`) — name of the auth header
  - `auth_scheme: String` (default `"Bearer "`) — prefix before the API key in the auth header value
- **New `AiProvider::spec()` trait method** with default `ProviderSpec::OpenAi` so any future external `AiProvider` implementor gets the OpenAI default without change. `ConfiguredProvider` overrides to return its stored spec.

### Changed
- **`complete()` (the OpenAI path) now uses the same configurable auth + headers** as `complete_anthropic`. Behaviour unchanged for every existing provider (defaults match the previous hardcoded `Authorization: Bearer`). Future OpenAI-compat providers can override if they need different auth.
- **Phase-1 startup panic on `spec = "anthropic"` removed.** That spec is now fully supported; the panic gate + its test are deleted.
- **`complete_with_cascade` dispatches by spec** (via the new `complete_dispatch` helper), so mixed OpenAI + Anthropic cascade chains work transparently — e.g. `[ai.fallback] on_censored = ["claude", "grok"]` tries Claude first then Grok when DeepSeek refuses.

### Tests
- ~29 new tests across: data-URL parsing, message translation (system extract, pass-through, image transform, tool_result wrap, bad-URL reject), tool-def translation, response parsing (single + multi text blocks, tool_use extraction, empty content, DSML embedded calls), schema parsing for the new fields (inline table + sub-table + custom auth + defaults), validation (empty auth_header, whitespace-only auth_header, empty header key, non-printable header value), and dispatch routing.
- Total inline unit-test count: 179 → 208.

### Phase 2 closure
- Phase 2 of #28 (Anthropic dispatcher + Claude provider) is complete. Issue #29 and umbrella issue #28 both close with this release.

## [0.15.0] - 2026-04-17

### Added
- **Config-driven AI providers** (#28, phase 1). Instance `config.toml` can now define custom providers (`[ai.providers.<name>]`) and override role routing (`[ai.routing]`). The four shipped providers (DeepSeek chat / DeepSeek Reasoner / Gemini / Grok) become a baked-in default registry — instances with no `[ai.*]` section behave bit-for-bit identically to 0.14.0.
- **Single-model setups supported.** Define one provider and route only `chat` to it; `vision` and `reasoner` gracefully degrade (image messages fall through to chat with a warning, classifier step is skipped). See `docs/configuration/ai-providers.md` for the worked example.
- **`defaults/example-providers.toml`** — copy-paste catalogue with annotated examples for Mistral, OpenAI, OpenRouter, Ollama localhost, Together AI, and Groq. Not loaded by the bot at runtime; pure discoverability.
- **`docs/configuration/ai-providers.md`** — full schema reference, default registry contents, validation rules table, worked examples.

### Changed
- **`AiProvider` trait `name`/`url`/`model` return `&str`** instead of `&'static str`. Required so `ConfiguredProvider` (the only impl now) can hand out references to its owned String fields. All callers pass these directly to `format!` / `tracing!` macros — unaffected.
- **`Config` no longer holds `deepseek_api_key` / `gemini_api_key` / `grok_api_key`** typed fields. The env vars are resolved by the new `ConfiguredProvider::from_def` via each provider's `api_key_env` field (default registry uses the same env var names — `DEEPSEEK_API_KEY`, `GEMINI_API_KEY`, `GROK_API_KEY`). No env-var name change.
- **Three per-provider files deleted:** `src/ai/providers/{deepseek,gemini,grok}.rs`. Replaced by `configured.rs` + `default_provider_registry()` in `mod.rs`.

### Fixed
- (none — strictly additive feature release)

### Tests
- Default-registry snapshot tests pin every field of every default provider against today's hardcoded values. Future drift fails the test.
- Schema parsing tests for `[ai.providers]` / `[ai.routing]` (empty, minimal user definition, optional fields, routing variants, fallback unchanged, unknown-field tolerance for phase 2 forward-compat).
- Validation tests covering all panic cases (typo in any role, routing without chat, whitespace in name, Anthropic spec) and graceful-degrade cases (unavailable referenced provider doesn't panic).
- Total inline unit-test count: 153 → 179 (+26 new across snapshot, parsing, merge, routing, and validation).

### Phase 2 (separate follow-up)
- Sub-issue filed for native Anthropic-spec dispatcher (`complete_anthropic`) + Claude provider. The `spec` field on every provider definition defaults to `"openai"` and is the dispatcher hook for phase 2; in 0.15.0 the bot panics at startup if any provider is configured with `spec = "anthropic"` so misconfigurations surface immediately.

## [0.14.0] - 2026-04-17

### Added
- **CENSORED cascade through alternate providers** (#13). When the primary AI provider returns its content-moderation refusal sentinel (DeepSeek's `"Content Exists Risk"` → `Err("CENSORED")`), the bot can now replay the same conversation through one or more alternate providers in order. First non-CENSORED success wins; if every entry also CENSORS, the existing snarky-reply canned message fires (preserves the refusal-as-feature behaviour for strict-moderation servers). Cascade is per-instance opt-in via a new `[ai.fallback] on_censored = ["grok", "gemini"]` field in `config.toml`. Default empty (no cascade).
- **Grok provider (xAI)**. New `GROK_API_KEY` env var (optional) and `Grok` provider in `src/ai/providers/grok.rs`. OpenAI-compatible endpoint, used as the obvious less-restrictive cascade target when DeepSeek refuses. Recognised by `[ai.fallback] on_censored` under the name `"grok"`. Not used as a primary provider.
- **`Data::ai_fallback_on_censored`** — the configured cascade name list, copied from `instance_config.ai.fallback.on_censored` at startup. Resolution against the router runs once at startup so unknown / unconfigured names log a warning during boot, not on every CENSORED.

### Changed
- **`handle_search_calls` and the three CENSORED detection sites in `src/ai/chat.rs`** route through the new `providers::complete_with_cascade` helper instead of `providers::complete` directly. When `[ai.fallback]` is unset (default), behaviour is identical to 0.13.2 — the helper short-circuits to the same single-provider call. When set, primary CENSORED → cascade through the resolved alts → first success returns, all CENSORED preserves the snarky-reply behaviour.
- Non-content errors from the primary (rate limits, network, 5xx) do **not** trigger the cascade — they're treated as transient and surfaced directly, since a different provider is unlikely to fix a transient issue. Cascade member errors (any error) cause the dispatcher to skip to the next alt.

### Tests
- 6 new unit tests for `ProviderRouter::named` and `cascade_for` (resolution behaviour, ordering, skipping unconfigured / unknown names, preserving duplicates). Total inline unit-test count: 147 → 153.

### Docs
- `docs/configuration/environment-variables.md` documents `GROK_API_KEY` and clarifies the role each AI key plays (DeepSeek primary chat, Gemini vision, Grok cascade-only).
- `instances/example/config.toml` includes a commented `[ai.fallback]` example.
- `instances/example/.env.example` includes `GROK_API_KEY=`.

## [0.13.2] - 2026-04-17

### Changed
- **AI providers behind an `AiProvider` trait** (#12). The orchestration in `src/ai/chat.rs` (renamed from `deepseek.rs` — the file was never DeepSeek-specific) used to build `ApiEndpoint { url, model, api_key }` literals at three call sites and route between them with model-name string compares inside `call_api`. Adding a non-OpenAI-compatible provider would have required a multi-site rewrite. New module `src/ai/providers/` exposes a metadata-only `AiProvider` trait (name / url / model / api_key / capability flags / max_tokens_limit / timeout) plus a free `complete()` function that does the OpenAI-compatible HTTP work against any `&dyn AiProvider`. `ProviderRouter` (built once at startup and held on `Data`) picks `vision()` / `chat()` / `reasoner()` based on capability flags. Behaviour is identical: same routing decisions, same per-provider caps, same reasoner pre-flight loop, same CENSORED handling. Future Anthropic support becomes a new file in `src/ai/providers/` plus a dedicated `complete_anthropic()` function — no churn at the call sites. Unblocks #13.

### Tests
- 7 new unit tests for `ProviderRouter` (key permutations) and the per-provider capability matrix + caps + timeout ordering. Total inline unit-test count: 140 → 147.

## [0.13.1] - 2026-04-17

### Fixed
- **MCP server panics on bind/serve failure** (#21). `mcp::start` previously called `.unwrap()` on `TcpListener::bind` and `axum::serve`, so a port collision (common in multi-instance Docker setups) or a transport error would panic the spawned task. The supervisor wrapper would catch and respawn forever. `mcp::start` now returns `Result<(), BotError>`; the security-gate refusal also returns `Err` instead of `panic!`. The caller in `events::ready` logs a single clean error and the rest of the bot keeps running.
- **README MCP tool count** (#20). The "22 Discord management tools" claim in the README was a stale carryover from earlier releases; the actual catalog has been at 51 since 0.13.0. Updated the count and added a link to `docs/reference/mcp-tool-catalog.md`.
- **README architecture diagram failed to render on GitHub**. Mermaid blocks with `<br/>` self-closing tags hang GitHub's renderer indefinitely. Switched to `<br>` so the diagram renders both in the GitHub README and in the mdBook.

### Changed
- Internal: extracted `is_censored_body` helper in `src/ai/deepseek.rs` for the DeepSeek `"Content Exists Risk"` sentinel-detection. Pure refactor (same substring check, same call site), made the helper testable.

### Tests
- `src/music/player.rs` (#22): 17 new unit tests for queue, loop modes, shuffle, and player state lifecycle. Was previously 0.
- `src/ai/deepseek.rs` (#23): 12 new unit tests for `get_system_prompt`, `is_bad_assistant_message`, and `is_censored_body`. Was previously 0 (the file is the largest in `src/`).
- `src/mcp/tools.rs` (#24): 4 new boundary tests for `parse_id` (negative, overflow, whitespace, u64 boundaries). Existing helper coverage was thorough; this rounds it out.
- Total unit-test count: 107 → 140.

## [0.13.0] - 2026-04-17

### Added
- **Configurable `command_root`** — new optional field in `instance config.toml` that controls the parent command name. Defaults to `"m"` (existing behaviour: `<prefix>m <subcommand>`); set to e.g. `"bot"` to differentiate two bots running in the same guild (`<prefix>m play` vs `<prefix>bot play`); set to `""` (empty) to skip the parent entirely and register every subcommand at the root (`<prefix>play`, `<prefix>skip`, ...). Validated at startup — must be a single token (whitespace rejected). The bot's pre-rendered command prefix is exposed as `Data::cmd_prefix` so the help command's example lines stay consistent across all three modes. (Closes #15)

### Added
- **MCP invite tools** (4): `list_invites`, `create_invite` (max_age / max_uses / temporary / unique), `delete_invite`, `get_invite_details` (server name, channel, member counts, expiration). (Closes #10)
- **MCP custom emoji tools** (4): `list_emojis`, `create_emoji` (fetches an HTTPS URL, base64-encodes, and uploads — rejects >256 KiB before the API call with a clear error), `edit_emoji` (rename), `delete_emoji`. (Closes #10)
- Brings the bot tool catalog to 51 (1 + 7 + 9 + 4 + 14 + 4 + 4 + 4 + 4). Closes out the SaseQ tool-gap series.

## [0.11.0] - 2026-04-17

### Added
- **MCP voice & stage tools** (6): `create_voice_channel` / `create_stage_channel` (voice-specialised companions to `create_channel` with bitrate, user_limit, etc.), `edit_voice_channel` (bitrate / user_limit / RTC region updates), and three voice-state tools — `move_voice_member` (drag user to a voice channel), `disconnect_voice_member` (kick from voice), `modify_voice_state` (server-mute / server-deafen). Pairs naturally with the existing music feature: ops can move the bot's voice channel via MCP without restarting the container. (Closes #8)
- Brings the bot tool catalog to 43 (1 + 7 + 9 + 4 + 14 + 4 + 4).

## [0.10.0] - 2026-04-17

### Added
- **MCP direct-message tools**: `send_private_message`, `read_private_messages`, `edit_private_message`, `delete_private_message`. Open (or reuse) a DM channel automatically; reads use REST so no `DIRECT_MESSAGES` privileged intent is required. Edits/deletes only work on messages the bot itself sent. (Closes #7)
- **MCP webhook tools**: `list_webhooks`, `create_webhook`, `delete_webhook`, `send_webhook_message`. The send tool supports per-message `username` / `avatar_url` overrides — the standard pattern for relay / persona / cross-platform-bridge bots. (Closes #9)
- Brings the bot tool catalog to 37.

### Changed
- Added `secrecy` as a direct dep (was already transitively pulled in by serenity 0.12). Used in `src/mcp/tools.rs` to read webhook tokens out of `Webhook::token`, which serenity wraps in `secrecy::Secret`.

## [0.9.0] - 2026-04-17

### Added
- **MCP message reactions**: `add_reaction` and `remove_reaction` tools accept unicode emoji, Discord custom-emoji format (`<:name:id>` / `<a:name:id>`), or a bare custom-emoji snowflake. `remove_reaction` targets the bot's own reaction only. (Closes #6)
- **MCP moderation rounding**: `set_nickname` (set/clear member nickname), `get_bans` (list active bans with paging), `remove_timeout` (lift an active timeout). Round out the existing moderation suite. (Closes #11)
- Brings the bot tool catalog to 29.

## [0.8.0] - 2026-04-16

### Added
- **`search_messages` MCP tool** — composable channel-message search. Filters: `author_id`, `author_name` (case-insensitive substring), `content` (case-insensitive substring), `after` and `before` (each accepts ISO 8601 dates like `2026-07-03` or numeric snowflakes). Pages backward from `before` (or now) until `limit` matches are found, the `after` boundary is reached, or the `max_pages` safety cap is hit. Returns matching messages plus a summary line stating how many were scanned and whether the search was truncated. Brings the bot tool catalog to 24.

### Added
- **`get_recent_messages` MCP tool** — read recent channel messages, newest first, with `limit` (1-100) and `before` for pagination. Brings the bot tool catalog to 23.

### Fixed
- **`mcp-gateway`: outgoing `Host` header override.** The bundled Compose deploy could never proxy MCP requests because rmcp's `StreamableHttpService` rejects non-loopback `Host` headers (DNS-rebinding protection); the gateway was sending `Host: <service-name>:9090` and getting back `403 Forbidden`. Gateway now sends `Host: localhost:9090` on every outgoing POST.
- **`mcp-gateway`: periodic tool list refresh.** The 5-minute background refresh only re-synced guild maps; the cached tool list was filled once at startup and never updated, so adding a new tool to a backend bot left it invisible to clients until the gateway itself was restarted. Now both `refresh_guild_map` and `refresh_tool_list` run on the periodic loop.

## [0.6.1] - 2026-04-16

Dependency-update sweep — applies the upgrades Dependabot was suggesting on the closed PR queue. No functional changes.

### Changed
- `mcp-gateway`: `reqwest` 0.12 → 0.13 (main crate stays on 0.12 until songbird 0.6 rebases off it)
- `toml` 0.8 → 1.1
- `rand` 0.8 → 0.10 (call sites migrated to `rand::rng()` and `RngExt::random_range`)
- All within-semver updates picked up by `cargo update` (notably `tokio` 1.50 → 1.52 in the gateway)
- GitHub Actions: `docker/login-action` v3 → v4, `softprops/action-gh-release` v2 → v3, `actions/upload-pages-artifact` v3 → v5

## [0.6.0] - 2026-04-16

First post-launch polish release. Focused on hardening, correctness fixes, and a real test suite. No breaking changes for existing deployments whose MCP server was already bound to `127.0.0.1` or already had `MCP_AUTH_TOKEN` set; see **Changed** for the bundled-Compose impact.

### Added
- `sqlx::migrate!` against a `migrations/` directory replaces the in-code `CREATE TABLE` block; per-schema `_sqlx_migrations` table tracks applied versions
- Panic recovery and graceful shutdown for supervised background tasks (`tokio::task::JoinSet` + `futures::FutureExt::catch_unwind`)
- Per-user cleanup task for every rate-limiter bucket; music, stocks, and moderation limiters are now enforced (previously defined but unwired)
- 92 unit tests (up from 37) and 18 Postgres-backed integration tests via `#[sqlx::test]`; CI runs both against a `postgres:17` service
- mdBook Mermaid preprocessor; the seven architecture diagrams now render instead of appearing as code blocks
- Remaining 25 documentation stub pages filled in (getting-started, configuration, features, architecture, deployment, development, reference)
- Quickstart step for generating the MCP auth token — the bundled Compose stack now requires it to start (see Changed)

### Changed
- `mcp-gateway` forwards its `MCP_AUTH_TOKEN` to each backend as the `Authorization: Bearer` header. Combined with the bot-side startup guard, the bundled Compose deploy is now a shared-secret model: `MCP_AUTH_TOKEN` in the instance `.env` and `MCP_GATEWAY_AUTH_TOKEN` in a repo-root `.env` must hold the same value.
- Bot refuses to start when `MCP_BIND_ADDR` is non-loopback *and* `MCP_AUTH_TOKEN` is empty; gateway refuses to start unconditionally without `MCP_AUTH_TOKEN`. Both comparisons are constant-time via the `subtle` crate.
- Stock portfolio columns moved from `DOUBLE PRECISION` to `NUMERIC(18, 4)`; Rust side uses `rust_decimal::Decimal` end-to-end so cents no longer drift over many fractional-share trades (migration in `20260414000001_stocks_decimal.sql`)
- Apache-style contributor grant replaces the prior inbound-license language in `CONTRIBUTING.md`
- Contribution terms + recent-change reconciliation pass across the docs

### Fixed
- **MCP:** 64 KiB body cap (`RequestBodyLimitLayer`); spec-compliant JSON-RPC parse error response; channel-targeting tools verify the channel belongs to the resolved guild before acting
- **Gateway:** dead pending-dispatcher code removed; startup panics instead of silently accepting empty auth
- **Stocks:** reset/buy/sell race closed with row-level locking; text-based confirmations replaced with a button that expires after one click
- **Autorole:** atomic-claim UPDATE prevents two concurrent event handlers from double-promoting the same member
- **Music:** Now Playing embed lifecycle consolidated so orphans don't accumulate after skip or loop transitions; skip no longer races the natural `TrackEnd` to advance twice; yt-dlp invocations have both a timeout and `kill_on_drop`
- **Welcome:** per-user rate limit replaces a global `Mutex` that serialised every join system-wide
- **Games:** Wordle and Connections refuse to overwrite an in-progress game; dates are validated and cross-checked against NYT's `print_date`; Wordle dictionary is a `HashSet` (was a `binary_search` on an unsorted `Vec`)
- **AI:** tool-search capped at three rounds (prompt said three, code allowed five); DSML closing tag accepts optional `/`; DSML strip scoped so prose isn't mangled; user-visible errors never leak raw upstream text
- **Chargeback:** staff role IDs moved from a hardcoded list into `config.toml`
- **Error surface:** `BotError` Display impls stop echoing internal error text; `format!("Database error: {e}")` wraps removed so sqlx's built-in context reaches logs
- **Docker:** build context now copies `migrations/` so `sqlx::migrate!` can compile-embed them
- **Polish:** `mistakes_dots` underflow guard; several places stop cloning an entire `Guild` to read one field

## [0.5.0] - 2026-04-14

Public launch on GitHub. No functional changes from 0.4.6 beyond repo hygiene, documentation, release automation, and cleanup of maintainer-specific state.

### Added
- Public repository launch with full documentation at <https://mrmcepic.github.io/discord-bot-rs/>
- AGPL-3.0-or-later license
- Hosted mdBook documentation covering getting started, configuration, features, architecture, deployment, and development (30+ pages of real content plus ~25 stubs)
- `CONTRIBUTING.md` with dev setup and inbound-license clause
- `SECURITY.md` with vulnerability reporting process
- `CODE_OF_CONDUCT.md` (Contributor Covenant v2.1)
- GitHub Actions CI: fmt, clippy (deny warnings), check, test, Docker build — for both the main crate and mcp-gateway
- GitHub Actions docs workflow: mdBook build and Pages deployment (landing page + book)
- GitHub Actions release workflow: multi-arch Docker image builds published to `ghcr.io`
- Dependabot config for cargo, GitHub Actions, and Docker ecosystems
- Issue and PR templates plus dependabot grouped updates
- Single-file HTML landing page at the Pages root
- Seven Mermaid architecture diagrams (top-level components, multi-instance topology, data flow, AI pipeline, music pipeline, MCP gateway routing, database ER diagram)

### Changed
- Generic parameterized `bot` service in `docker-compose.yml` (was hardcoded `examplebot`/`secondbot`)
- `INSTANCE_DIR` env var controls which instance directory the bot mounts
- `INSTANCES` env var controls MCP gateway routing table
- Example instance (`instances/example/`) expanded into a fully documented config reference
- Rust sources formatted with `hard_tabs = true, tab_spaces = 4` (one-time reformat)
- Help command now uses the per-instance `bot_name` from `config.toml` instead of a hardcoded string
- README rewritten as a showcase landing with badges, feature cards, architecture diagram, and documentation links

### Removed
- Hardcoded `OWNER_ID` constant and the check-and-return branches that protected a specific user from `!m ban` and AI tempban tool calls
- `version_info.txt` and its `include_str!` reference in the AI system prompt
- `ecosystem.config.cjs` (stale PM2 leftover from a previous deployment)
- `instances/examplebot/` and `instances/secondbot/` (maintainer's private configs, moved to a private fork)
- `docs/superpowers/` internal design specs and plans (moved to private fork)
- Hardcoded test-fixture instance names and guild IDs in `mcp-gateway` tests

### Fixed
- 62 pre-existing clippy warnings across both crates now pass `cargo clippy --all-targets -- -D warnings`

## [0.4.6] - 2026-04-14

Bootstrap entry. This release represents the project state at the time of public launch. Features are grouped by subsystem.

### Added

**Core**
- Multi-instance architecture: one binary runs any number of bot instances, each with its own config directory, database schema, personality, and feature set
- Per-instance PostgreSQL schema isolation
- Graceful shutdown and idle-timer based voice disconnect

**AI Chat**
- DeepSeek V3.2 primary provider (`deepseek-chat`)
- Google Gemini fallback provider
- @mention activation in any channel the bot can read
- 30-minute / 10-message conversation context window
- Personality file as system prompt (`personality.txt` per instance)
- Response sanitization and AI tool support (web search, confirmation workflow)

**Music**
- yt-dlp + ffmpeg audio pipeline with OGG/Opus passthrough (256 kbps)
- Queue with loop, shuffle, skip, and previous
- Interactive button controls on the "now playing" embed
- Auto-leave on empty voice channel

**Games**
- Wordle (daily word guess)
- Connections (group-by-category)
- Virtual stock trading with real-time Finnhub data

**Moderation**
- Slash commands for ban, kick, timeout, role assignment

**Member Join**
- Optional join role assignment for new members
- Optional AI-generated welcome message with rate limiting
- Auto-role promotion based on age-in-guild and message-count criteria

**Minecraft Module**
- `!m verify` command to link Discord to a Minecraft account via companion plugin
- Donator sync: polls MC server and syncs supporter/premium Discord roles
- Chargeback alerts: webhook-driven role stripping and interactive staff alert flow

**MCP Server**
- Embedded Model Context Protocol server for programmatic Discord management
- 22 tools covering guilds, channels, roles, members, messages
- Supports Claude Code via HTTP transport
- Optional bearer-token authentication

**MCP Gateway**
- Separate service that routes MCP requests across multiple bot instances
- Streamable HTTP SSE protocol support
- Per-instance session management

**Deployment**
- Docker Compose setup with bundled PostgreSQL
- Health checks for all services

### Known Issues
- AI context bleed: the message history builder can mix context from concurrent conversations in the same channel. Tracked in [#1](https://github.com/MrMcEpic/discord-bot-rs/issues/1).
- MCP OAuth: bearer-token auth works but Claude Code's HTTP transport expects OAuth 2.1. Currently running without auth on localhost only. Tracked in [#2](https://github.com/MrMcEpic/discord-bot-rs/issues/2).

[Unreleased]: https://github.com/MrMcEpic/discord-bot-rs/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/MrMcEpic/discord-bot-rs/releases/tag/v0.5.0
[0.4.6]: https://github.com/MrMcEpic/discord-bot-rs/releases/tag/v0.4.6
