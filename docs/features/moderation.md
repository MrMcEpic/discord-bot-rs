# Moderation

A small, focused set of moderation commands: temporary bans with
automatic expiry, an early-unban override, an active-tempbans listing,
and a bulk-message-delete ("nuke") tool. Every action can optionally be
mirrored to an audit-log channel.

## What it does

The moderation module is deliberately narrow. It covers the cases
where a Discord-native action falls short:

- **Tempban** — Discord's built-in ban is permanent; the bot adds an
  expiry timestamp and a background task that automatically lifts the
  ban when the duration is up.
- **Unban** — early termination of a tempban (or removal of any
  Discord ban), with database bookkeeping to mark the active record
  as resolved.
- **Banlist** — a quick view of every active tempban in the guild,
  with the user, moderator, expiry timestamp (relative), and reason.
- **Nuke** — bulk delete the most recent N messages in a channel.
  Useful for cleaning up spam raids and accidental message dumps.

There is no kick command, no timeout command, and no role-assignment
command in the moderation module. Discord's built-in tools cover
those well enough that wrapping them adds no value. Auto-role
promotions and join-role assignment are separate features (see
[Auto-Role](auto-role.md) and [Join Features](join-features.md)).

Moderation is always-on. There's no `[features]` flag and no
configuration required — it works out of the box. The audit-log
channel is the one optional bit, configured at runtime per guild.

## Commands

All commands live under the `m` parent. With the default `!`
prefix that means **`!m <subcommand>`**. The prefix is configurable
per instance.

| Command | Required Permission | Description |
|---|---|---|
| `!m ban <@user> <duration> [reason]` | `BAN_MEMBERS` | Tempban a user. Duration is a short string (`30s`, `5m`, `2h`, `3d`, `1w`); reason is `#[rest]`, so spaces are fine. |
| `!m unban <@user>` | `BAN_MEMBERS` | Lift any ban on the user. Marks an active tempban resolved if one exists. |
| `!m banlist` (alias `!m bans`) | `BAN_MEMBERS` | Show all active tempbans in this guild with expiry timestamps. |
| `!m nuke <count>` | `MANAGE_MESSAGES` | Bulk-delete the last `count` messages in the channel. `count` must be 1-100. |
| `!m setlog <#channel>` | `ADMINISTRATOR` | Set the audit-log channel for this guild. (Lives in the admin command module, but enables moderation logging.) |

Permissions are enforced by Poise's `required_permissions`
attribute — invocations from users without the listed permission are
rejected before the command body runs, so unprivileged users get
Discord's standard "missing permissions" error rather than the bot's
own message.

## Tempbans in detail

### Duration parsing

The `duration` argument uses the same short-string format as
`min_age` for [auto-role](auto-role.md): `<integer><unit>` with no
spaces, where unit is `s`, `m`, `h`, `d`, or `w`. Examples:

- `30s` — 30 seconds
- `15m` — 15 minutes
- `2h` — 2 hours
- `3d` — 3 days
- `1w` — 1 week

Combined units (`1d12h`) are not supported. The maximum is 365 days;
anything longer is rejected with "Invalid duration".

### What `!m ban` actually does

1. Parses the duration; bad input gets
   `Invalid duration. Use: 30s, 5m, 2h, 3d, 1w`.
2. Inserts a row into the `tempbans` table with the guild, user,
   moderator, expiry timestamp, and reason.
3. Calls Discord's `ban_with_reason` with the audit-log reason
   `Tempban by <moderator> (<duration>): <reason>`.
4. Replies in the invocation channel with a relative-timestamp
   footer.
5. If an audit-log channel is configured, posts a richer embed
   there — see [Audit logging](#audit-logging).

### Automatic expiry

A background task runs every **30 seconds** (started about 5
seconds after bot boot), polling the `tempbans` table for entries
whose `expires_at` is in the past. For each expired entry, the
bot calls `remove_ban` against Discord's API and marks the row
as `unbanned` in the database. The audit-log reason on the unban
call is "Tempban expired".

This means there's a worst-case 30-second window between a
tempban's nominal expiry and the user actually being unbanned.
Good enough; nobody really notices a tempban resolving 25 seconds
late.

### Early unban

`!m unban @user` calls Discord's unban API and updates the
database. If there was no active tempban for that user in the
database, the bot still issues the Discord unban (so it works
for non-tempban bans too) and appends "(No active tempban was
found in the database.)" to the reply, so the moderator knows.

### Banlist

`!m banlist` queries the `tempbans` table for everything still
flagged active in this guild and renders an embed with one line
per ban: the user mention, a relative expiry timestamp, the
moderator who issued it, and the reason if there was one.

Empty output is "No active tempbans."

## Nuke (bulk delete)

`!m nuke <count>` fetches the last `count + 1` messages in the
channel (the `+ 1` is to swallow the command invocation itself),
then bulk-deletes them. Discord's bulk-delete endpoint refuses
messages older than 14 days, so:

- The actual delete count may be lower than `count` if some of the
  recent messages are old.
- The reply tells you how many actually got deleted: `Deleted
  **42/100** messages (messages older than 14 days can't be
  bulk-deleted).`

The bot's own confirmation message is auto-deleted after 3 seconds
so it doesn't sit in the channel. The audit-log embed (if
configured) records the action.

The minimum is `1`, the maximum is `100`. Anything outside that
range gets a usage hint instead of running.

## Audit logging

Every successful moderation action — tempban, unban, nuke — can be
mirrored to a designated channel as a structured embed. Set it up
once per guild:

```
!m setlog #mod-actions
```

Requires `ADMINISTRATOR`. The channel ID is stored in the
`guild_settings.audit_log_channel_id` column. Subsequent moderation
actions look up that channel and post:

- **Tempban / Nuke** — red border (`#ed4245`), title `Mod Action:
  <Tempban|Nuke>`, fields for user/moderator/duration (or channel
  + count), timestamped.
- **Unban** — green border (`#57f287`), same shape.

If no audit-log channel is set, the action still happens — the
audit post is silently skipped.

Audit-log delivery is best-effort: if the bot can't post to the
configured channel (deleted, bot lacks permission, etc.) the
moderation action still goes through and the failure is swallowed.

## AI moderation

The AI chat layer can invoke moderation tools (`tempban`, `unban`,
`nuke`) on behalf of a user. Every AI-initiated moderation call
goes through a confirmation embed with **Approve** and **Cancel**
buttons (only the original requester can press them, requester's
Discord permissions still checked, expires after 30 seconds). See
[AI Chat: Moderation confirmation](ai-chat.md#moderation-confirmation).

## Permissions required

The bot itself needs `BAN_MEMBERS` (for `!m ban` and
`!m unban`) and `MANAGE_MESSAGES` (for `!m nuke`). Moderators
need the same permissions on their own role to invoke the
commands.

For `!m setlog` the moderator needs `ADMINISTRATOR`. The bot
needs `SEND_MESSAGES` and `EMBED_LINKS` in the audit-log channel
itself.

## Common issues

- **"Invalid duration"** — your duration string didn't parse.
  Use `30s`, `5m`, `2h`, `3d`, `1w`. Combined formats aren't
  supported.
- **Tempban issued, user comes back after a bot restart** —
  shouldn't happen. The database is the source of truth and
  the unban task picks up where it left off.
- **`!m banlist` empty after a restart** — only if you wiped
  the database. The Discord ban list is independent (Server
  Settings → Bans).
- **Nuke deleted fewer messages than asked** — Discord's
  14-day bulk-delete limit; the bot reports the actual count.
- **Audit log embed missing** — wrong channel ID, missing
  permissions, or `!m setlog` never ran.
- **Unban says "No active tempban was found"** — not a bug;
  the user was banned manually and there's no tempban row to
  mark resolved. The unban itself worked.

## Cross-references

- [Auto-Role](auto-role.md) — the role-promotion side of moderation
  tooling.
- [Join Features](join-features.md) — auto-assigning a role on
  join, and AI welcome messages.
- [AI Chat: Moderation confirmation](ai-chat.md#moderation-confirmation) —
  how the AI gates moderation tool calls behind a button confirmation.
- [Instance Config](../configuration/instance-config.md) — for
  `command_prefix`.
