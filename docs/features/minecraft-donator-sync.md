# Minecraft: Donator Sync

Periodically poll a Minecraft server's donator list and reconcile two
Discord roles — `supporter_role` and `premium_role` — so that the
right people have the right perks at any moment, without anyone
having to manually grant or revoke them.

The companion to this feature is the
[chargeback](minecraft-chargeback.md) flow, which handles the
reverse case: someone files a chargeback, their Discord roles get
stripped, and a staff alert is posted.

## What it does

The donator sync background task wakes up every `check_interval`
seconds, calls the Minecraft companion plugin's `/api/donators`
endpoint, and reconciles the response with the current state of the
guild. After each tick:

- Anyone who's a **supporter** on the MC server but doesn't have
  the supporter role gets it.
- Anyone who's a **premium** donator but doesn't have the premium
  role gets it.
- Anyone who has either role on Discord but isn't on the MC list
  loses it (their donation expired or was revoked).
- Tier upgrades (supporter → premium) and downgrades (premium →
  supporter) are handled correctly: the old role is removed when
  the new one is added.
- Anyone with the chargeback **restricted role** is skipped
  entirely — sync never re-grants donor perks to a user being
  held by a chargeback alert.

The sync is **bidirectional state reconciliation**, not an event
stream. There's no "donator added" notification; the bot just
queries the source of truth and patches the difference.

## Activation

Donator sync is part of the Minecraft module. You need:

```toml
[features]
minecraft = true

[minecraft]
donator_sync = true

[minecraft.donator_sync_config]
supporter_role = "123456789012345678"
premium_role   = "234567890123456789"
check_interval = 300                        # seconds; default 300 (5 min)
```

Plus, in `.env`:

```
MC_VERIFY_URL=https://your-mc-server.example.com
MC_VERIFY_SECRET=long-random-string-shared-with-the-mc-plugin
```

(The same `MC_VERIFY_*` pair as [verify](minecraft-verify.md) and
[chargeback](minecraft-chargeback.md). One pair, three sub-features.)

Restart the bot. The startup log will show:

```
Donator sync enabled (supporter=…, premium=…, interval=300s)
Donator sync checker started (300s interval).
```

The first sync runs about 15 seconds after bot boot, then every
`check_interval` seconds after that.

If `donator_sync = true` but the `[minecraft.donator_sync_config]`
section is missing, or `MC_VERIFY_URL`/`MC_VERIFY_SECRET` aren't
set, the bot logs a warning and the sync task never spawns.

## Configuration

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `supporter_role` | string (snowflake) | yes | — | Role ID granted to users with the `supporter` tier. |
| `premium_role` | string (snowflake) | yes | — | Role ID granted to users with the `premium` tier. |
| `check_interval` | integer | no | `300` | Seconds between syncs. Lower = fresher; higher = less load. |

Both role IDs must be valid snowflakes (numeric strings). Invalid
IDs cause the entire sync to abort with a logged warning rather
than partially executing.

The default `check_interval` of 5 minutes is conservative. For most
deployments that's fine — donations don't change often, and the
five-minute lag is invisible to users. Push it lower (e.g. `60`)
only if you have a use case that needs faster sync.

## How it works

`src/minecraft/donator_sync.rs::sync_roles` runs in five logical
phases per tick:

1. **Fetch.** Call `<MC_VERIFY_URL>/api/donators` with the
   shared secret. Response shape:

   ```json
   {
     "donators": [
       { "discord_id": "987654321098765432", "tier": "supporter" },
       { "discord_id": "234567890123456789", "tier": "premium" }
     ]
   }
   ```

2. **Build target sets.** Walk the response and produce two sets:
   `should_have_supporter` and `should_have_premium`. Unknown
   tiers (anything that isn't `supporter` or `premium`) are
   logged as warnings and ignored.

3. **Snapshot current state.** Call Discord's
   `get_guild_members` (paginated, 1000 per page) and build
   three sets: `current_supporters`, `current_premium`, and
   `restricted_users` (anyone with the chargeback-restricted
   role, if chargeback is also configured).

4. **Add missing roles.** For each user who should have a tier
   but doesn't, call `add_member_role` with the audit-log
   reason `Donator sync: <tier> tier`. Restricted users are
   skipped.

5. **Remove stale roles, then handle tier transitions.** Anyone
   who has a tier role but isn't on the MC list loses the role
   ("Donator sync: <tier> expired"). Anyone whose tier moved
   gets the old role removed ("upgraded to premium" or
   "downgraded to supporter").

The sync is **idempotent** — running it twice in a row does no
extra work the second time, since the desired and current states
already match.

### The restricted-role short-circuit

If the chargeback feature is configured, donator sync also reads
`chargeback_config.restricted_role` and treats it as a "do not
touch" set. Any member with that role is skipped during all five
phases. The reasoning: a user under a chargeback hold should not
have donor perks restored automatically just because the MC
server hasn't yet pulled them off its donator list.

If chargeback isn't configured, no restriction set is built and
this short-circuit doesn't apply.

### Pagination

Guilds with many members are paginated 1,000 at a time. The loop
breaks once a page returns fewer than 1,000 entries (the standard
sentinel for "end of list"). For most servers this is a single
page; for very large guilds it's a handful of API calls per tick.

## Permissions

The bot's role must:

- Be **higher** than both `supporter_role` and `premium_role` in
  the role hierarchy.
- Have `MANAGE_ROLES`.

Without those, individual `add_member_role` / `remove_member_role`
calls fail with permission errors and get logged at `warn`. The
sync continues to the next user; one bad permission on one role
won't break the whole loop.

## Common issues

- **No sync log line at startup** — `donator_sync = true` but
  either the config sub-section or `MC_VERIFY_*` is missing.
  Check the warning log lines around startup.
- **Sync runs but no roles change** — the MC server's
  `/api/donators` returned an empty or malformed response. Check
  the bot's logs for `MC server returned status …` or
  `Invalid donator response`. Try `curl
  -H "Authorization: Bearer $MC_VERIFY_SECRET"
  $MC_VERIFY_URL/api/donators` from the bot host.
- **Roles get added then removed seconds later** — the MC server
  is returning inconsistent data between calls (e.g. an unstable
  cache). Pin down whether the source data is stable; the sync
  is idempotent on stable input.
- **Restricted-role users are losing their donor role** — that's
  the intended behaviour during a chargeback hold. Roles are
  *not* removed for them — they're just *not added* by sync. To
  restore them, lift the chargeback restriction first.
- **`Invalid supporter_role ID` warning** — one of the snowflake
  fields isn't a numeric string. Snowflakes are quoted strings
  in TOML; make sure you have `supporter_role = "123…"` and
  not `supporter_role = 123…` (TOML integers can't represent
  Discord snowflakes accurately).
- **Sync feels slow** — `check_interval` is 5 minutes by
  default. Lower it if you need faster reconciliation, but bear
  in mind every tick fetches the full member list from Discord
  and the donator list from MC.
- **Tier change took effect on MC but Discord lags** — bounded by
  the next `check_interval` tick. There's no event hook for
  faster propagation.

## Cross-references

- [Minecraft: Verify](minecraft-verify.md) — required precursor
  for any Discord ↔ MC mapping.
- [Minecraft: Chargeback Alerts](minecraft-chargeback.md) — the
  inverse flow; donator sync reads the restricted role this
  feature applies.
- [Instance Config: `[minecraft.donator_sync_config]`](../configuration/instance-config.md#minecraftdonator_sync_config-section) —
  schema reference.
- [Environment Variables](../configuration/environment-variables.md) —
  `MC_VERIFY_URL`, `MC_VERIFY_SECRET`.
