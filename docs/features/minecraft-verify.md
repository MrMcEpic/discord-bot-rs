# Minecraft: Verify

Link Discord accounts to Minecraft accounts. The user generates a
verification code on the Minecraft server, types it into Discord with
`!m verify`, and the bot calls the Minecraft server's HTTP API to
confirm the link.

This is the foundational sub-feature of the Minecraft module. The
[donator sync](minecraft-donator-sync.md) and
[chargeback](minecraft-chargeback.md) features both rely on the
Discord ↔ Minecraft mapping that verify produces.

## What it does

A typical flow:

1. A player joins your Minecraft server and runs an in-game
   `/verify` command. The companion plugin generates a short
   alphanumeric code, stores it server-side with a TTL, and tells
   the player to type `!m verify <code>` in Discord.
2. The player runs `!m verify <code>` in any Discord channel where
   the bot can read messages.
3. The bot POSTs the code plus the player's Discord ID to the
   Minecraft server's `/api/verify` endpoint.
4. The Minecraft server validates the code, links the Discord ID
   to the player's Mojang UUID, and responds with success +
   username.
5. The bot replies in Discord with
   `Verified! Your Discord account is now linked to **<username>**
   in Minecraft.`

Once linked, that mapping powers everything else the Minecraft
module does — donator role sync, chargeback alerts, and any future
features that need to know who-is-who across the two platforms.

## Activation

Verify is part of the Minecraft module. Three things have to line up
in `config.toml`:

```toml
[features]
minecraft = true

[minecraft]
verify = true   # default; can be omitted

[minecraft.donator_sync_config]
# unrelated, but [minecraft] needs to exist
```

And two environment variables in `.env`:

```
MC_VERIFY_URL=https://your-mc-server.example.com
MC_VERIFY_SECRET=long-random-string-shared-with-the-mc-plugin
```

`MC_VERIFY_URL` is the base URL of the Minecraft companion plugin's
HTTP server (no trailing slash needed; the bot strips it). The bot
appends `/api/verify` for the verify call.

`MC_VERIFY_SECRET` is a shared bearer token. The bot sends it as
`Authorization: Bearer <secret>`; the plugin must compare against
the same value. Treat it like a password — anyone with this secret
can issue verify (and ban) calls against the MC server.

`verify` defaults to `true` inside the `[minecraft]` section, so
enabling the Minecraft module is enough to enable verify. Set
`verify = false` only if you want donator sync or chargeback
without exposing the verify command.

The `!m verify` command is registered conditionally at startup —
if `features.minecraft` is `false` or `minecraft.verify` is
`false`, the command isn't added to the parent command tree at
all. Users typing `!m verify` see the standard "command not
found" response.

## Commands

| Command | Description |
|---|---|
| `!m verify <code>` | Submit a verification code from the Minecraft server. The code is uppercased before being sent (the MC plugin's codes are case-insensitive). The argument is `#[rest]`, so trailing whitespace is fine. |

If the user runs `!m verify` with no code, the bot replies:

> Usage: `!m verify <code>` — get your code by running `/verify`
> in Minecraft.

If `MC_VERIFY_URL` or `MC_VERIFY_SECRET` is missing from the
environment, the command refuses with:

> MC verification is not configured. Set `MC_VERIFY_URL` in .env

(or the same about `MC_VERIFY_SECRET`).

## How it works

`src/minecraft/api.rs::verify` is a thin POST against
`<MC_VERIFY_URL>/api/verify` with the body:

```json
{
  "code": "ABC123",
  "discord_id": "987654321098765432"
}
```

and the `Authorization: Bearer <MC_VERIFY_SECRET>` header. The
Minecraft plugin is expected to respond with JSON of the shape:

```json
{ "success": true,  "username": "Steve", "uuid": "069a79f4-..." }
```

or, on failure:

```json
{ "success": false, "error": "Invalid or expired code" }
```

The bot turns the success path into a Discord confirmation message
including the linked Minecraft username. On `success: false` it
echoes the `error` string back to the user, so the plugin's error
copy ("expired", "code already used", etc.) reaches the player
unchanged. On HTTP/transport failure the bot replies
`Could not reach the MC server: <error>` so the user knows the
linkage didn't happen because of infrastructure rather than a bad
code.

The bot does **not** store the linkage itself. The Minecraft side is
the source of truth. Other Minecraft sub-features
(donator sync, chargeback) re-fetch the mapping from the same
plugin when they need it.

## The Minecraft side

This module only describes the Discord side of the integration.
The companion plugin on the Minecraft server is its own thing —
ours implements `/api/verify`, `/api/donators`, and `/api/ban`
endpoints, plus a chargeback webhook that POSTs to the bot. If
you're rolling your own integration, the contract is:

- `POST /api/verify` — accepts `{code, discord_id}`, returns
  `{success, username, uuid, error}`. Auth via bearer token.
- The plugin is responsible for issuing codes, validating them,
  enforcing TTLs, and persisting Discord ↔ UUID mappings.

For the source-of-truth wire format, see
`src/minecraft/api.rs` in the bot codebase.

## Permissions

The bot needs no special Discord permissions for verify itself —
just the standard `SEND_MESSAGES` to reply in the channel where
the command was invoked. There's no role assignment step on
successful verification; that's left to the plugin (which can
trigger donator sync separately) or to a server admin.

## Common issues

- **"MC verification is not configured"** — `MC_VERIFY_URL` or
  `MC_VERIFY_SECRET` is missing from the running environment.
  Make sure your `.env` is being passed in (Docker users:
  `env_file: .env` in `compose.yaml`).
- **"Could not reach the MC server"** — the bot can't contact the
  URL. Check that `MC_VERIFY_URL` is reachable from the bot's
  network (try `curl` from inside the bot container), and that
  the plugin's HTTP listener is up.
- **"Verification failed: invalid or expired code"** — the
  Minecraft plugin rejected the code. The most common causes
  are typos, codes that have already been used, or codes that
  TTL'd out (most plugins expire codes after a few minutes).
- **`401 Unauthorized` in bot logs** — `MC_VERIFY_SECRET` doesn't
  match the value the plugin expects. Make sure both sides have
  the exact same string (no trailing whitespace).
- **`!m verify` says "command not found"** — `features.minecraft`
  or `minecraft.verify` is `false`, or the bot wasn't restarted
  after enabling them.
- **The verification succeeded, but donator roles still aren't
  syncing** — verify only creates the mapping. Donator role
  assignment is a separate sub-feature; see
  [Minecraft: Donator Sync](minecraft-donator-sync.md).

## Cross-references

- [Minecraft: Donator Sync](minecraft-donator-sync.md) — uses
  the Discord ↔ UUID mapping that verify produces.
- [Minecraft: Chargeback Alerts](minecraft-chargeback.md) —
  reaches out from the MC server to Discord using the same
  mapping.
- [Instance Config: `[minecraft]`](../configuration/instance-config.md#minecraft-section) —
  schema reference.
- [Environment Variables](../configuration/environment-variables.md) —
  `MC_VERIFY_URL`, `MC_VERIFY_SECRET`.
- [Secrets Management](../configuration/secrets-management.md) —
  how to keep the shared secret out of source control.
