# Auto-Role Promotion

Promote members from one role to another automatically once they cross a
configurable activity threshold. Useful for unverified → verified
flows, lurker → member graduations, and any other "stick around long
enough and you get the upgrade" pattern.

## What it does

The auto-role module watches every member who currently holds a
designated `from_role` and promotes them — adding `to_role` and
removing `from_role` — once they meet your criteria. The criteria are
simple:

- **Minimum age** in the guild (e.g. `3d`).
- **Minimum messages** sent in the guild (e.g. `20`).

You decide whether either condition is enough to trigger promotion
(`require_all = false`, the default), or whether **both** must hold
(`require_all = true`).

Two checks run in parallel so promotions feel snappy without depending
solely on activity:

1. **Live, on every message.** When a member posts in the guild, the
   bot increments their message count and immediately checks whether
   they now meet the threshold. If so, the promotion fires
   asynchronously.
2. **Background, every 60 seconds.** A scheduled task scans every
   unpromoted member, including ones who've gone quiet, and promotes
   anyone whose age-in-guild has crossed the line.

Together this means a chatty user is promoted within a few seconds of
hitting the message threshold; a silent user is promoted within a
minute of crossing the age threshold.

## Activation

Auto-role is opt-in. Enable it in `config.toml`:

```toml
[features]
auto_role = true

[auto_role]
from_role    = "123456789012345678"   # the role to remove
to_role      = "987654321098765432"   # the role to grant
min_age      = "3d"
min_messages = 20
require_all  = false
```

Restart the bot after editing. The startup logs will show:

```
Auto-role module enabled (from=…, to=…, min_age=3d, min_messages=20, require_all=false)
Auto-role time checker started (60s interval).
```

If you set `features.auto_role = true` but omit the `[auto_role]`
section, the bot logs a warning and the feature stays off — it
doesn't refuse to boot. See
[`docs/configuration/instance-config.md`](../configuration/instance-config.md#auto_role-section)
for the field reference.

## Configuration

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `from_role` | string (snowflake) | yes | — | The role the member starts with. Removed on promotion. |
| `to_role` | string (snowflake) | yes | — | The role the member ends up with. Added on promotion. |
| `min_age` | duration string | no | `"3d"` | Minimum time in the guild before promotion. |
| `min_messages` | integer | no | `20` | Minimum messages in the guild before promotion. |
| `require_all` | bool | no | `false` | If `true`, both criteria must hold; if `false`, either does. |

### Snowflake IDs, not role names

Both `from_role` and `to_role` are **role IDs** as strings (Discord
snowflakes), not role names. To get a role's ID: in Discord,
turn on Developer Mode (Settings → Advanced → Developer Mode), then
right-click the role in Server Settings → Roles and choose "Copy
ID".

### Duration format

`min_age` accepts a single integer-with-unit string: `30s`, `15m`,
`2h`, `3d`, `1w`. No combined units. Maximum 365 days. The default
is `3d`.

### Combining criteria

With `require_all = false` (the default), the bot promotes as soon
as **either** condition holds. A user who's been in the guild for
four days but has never spoken still gets promoted on the next
60-second tick. A user who joined ten minutes ago and has already
sent 30 messages gets promoted the moment their 20th message lands.

With `require_all = true`, both conditions must hold. Same flow,
but neither condition alone is enough to fire the promotion.

## How it works

### Tracking activity

The first time the bot sees a message from a user in a guild with
auto-role enabled, it inserts a row into the `member_activity`
table with `message_count = 1` and `first_seen = NOW()`. Every
subsequent message increments `message_count`. The `promoted` flag
on the row starts as `FALSE` and flips to `TRUE` after a successful
promotion.

`first_seen` is set on the **first observed message**, not on the
member's actual join timestamp. A user who joined the guild before
auto-role was enabled — or before the bot was running — is treated
as "first seen" the next time they post. This is intentional: the
bot can't promote someone it has no record of, and falling back to
join time would risk batch-promoting half the server the moment
you enable the feature.

If you want to backfill historical members, the cleanest path is
to manually add `to_role` to existing members and let the auto-role
flow only handle newcomers.

### The two promotion paths

**Live path** (`src/events/mod.rs::handle_message`): on every
non-bot guild message, the bot increments the user's count and
immediately checks `meets_criteria`. If true, it spawns a task
that calls `try_promote`. This is where the message-count
threshold gets caught.

**Background path** (`src/main.rs`): a long-lived task wakes up
every 60 seconds, queries the `member_activity` table for every
row with `promoted = FALSE` in the configured guild, evaluates
each one against the criteria, and promotes whoever qualifies.
This is where the age-only path gets caught.

The two paths converge on the same `try_promote` function, which:

1. Adds `to_role` to the member.
2. Removes `from_role`.
3. Sets `promoted = TRUE` in the database.

Both Discord API calls have the audit-log reason "Auto-role
promotion", so the action is identifiable in the guild's audit
log.

### Multi-instance / multi-guild scope

Auto-role is configured per **bot instance**, not per guild. The
background task uses the `GUILD_ID` env var to know which guild
to scan. If you want different thresholds in different guilds,
run separate bot instances with separate `config.toml` files. See
[Multiple Instances](../configuration/multiple-instances.md).

## Permissions

The bot's role must:

- Be **higher in the role list** than both `from_role` and
  `to_role` (Discord enforces role-hierarchy on add/remove).
- Have the **Manage Roles** permission.

Without either, the promotion call will fail and the bot will log
`Auto-role promotion failed: …` for that user. The row stays
`promoted = FALSE`, so the next tick will retry — possibly
forever, until you fix the permissions.

## Common issues

- **Nobody is being promoted** — confirm the startup log shows
  `Auto-role module enabled` and `Auto-role time checker
  started`. If only the first, the `[auto_role]` section is
  missing or the role IDs failed to parse.
- **One user stuck at `promoted = FALSE`** — bot's role isn't
  above both target roles, or it lost `MANAGE_ROLES`. Check
  the logs around the promotion attempt.
- **Existing members aren't being promoted** — expected. The
  scan only sees members who have a `member_activity` row,
  created on their first observed message. Silent members
  stay invisible until they post.
- **Promotion is late** — bounded by the 60-second background
  tick for age-only promotions. The live (message-count) path
  is near-instant.
- **Re-promote a user who lost the role** — flip `promoted`
  back to `FALSE` on their `member_activity` row in the
  database; the next tick re-promotes.
- **`Invalid from_role ID` warning** — value isn't a numeric
  snowflake. Copy the role's ID, not its name.

## Cross-references

- [Instance Config: `[auto_role]`](../configuration/instance-config.md#auto_role-section) —
  full schema reference.
- [Join Features](join-features.md) — for auto-assigning a role
  the moment someone joins, before they hit the activity
  threshold.
- [Moderation](moderation.md) — for tempbans and bulk delete,
  the manual side of mod tooling.
- [Multiple Instances](../configuration/multiple-instances.md) —
  how to run different auto-role configs in different guilds.
