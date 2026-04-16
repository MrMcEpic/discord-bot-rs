# Minecraft: Chargeback Alerts

When a player files a chargeback against a purchase on your Minecraft
server, the bot reacts immediately: it strips the player's Discord
roles, applies a "restricted" role, and posts a staff alert with
**Ban** and **Dismiss** buttons. Staff can ban (Discord + Minecraft)
or dismiss with a click.

This closes the loop on the donor flow:
[verify](minecraft-verify.md) creates the Discord ↔ Minecraft
mapping, [donator sync](minecraft-donator-sync.md) keeps roles in
line with active donations, and chargeback alerts handle the case
where a donation gets reversed.

## What it does

The MC store (Tebex, Buycraft, or whatever you use) is wired up to
POST a chargeback notification to the bot's HTTP listener at
`/webhook/chargeback`. On receipt:

1. **Authenticates** the request via `Authorization: Bearer
   <MC_VERIFY_SECRET>`. Mismatched or missing tokens get a
   `401 Unauthorized` and no further processing.
2. **Strips and restricts.** If the chargeback payload includes a
   linked `discord_id`, the bot calls `edit_member` on Discord
   with a fresh roles list of just `[restricted_role]` —
   wiping every other role the user had — and an audit-log
   reason `Chargeback: roles stripped, user restricted`.
3. **Posts a staff alert** to the configured staff channel,
   showing the player's MC username, tier, UUID, the linked
   Discord account (if any), and a timestamp. Two buttons sit
   at the bottom: 🔨 **Ban** and ❌ **Dismiss**.
4. **Returns 200** to the MC store so the chargeback notification
   isn't retried.

The Ban button issues a Discord ban (if the user is linked) and
a Minecraft ban (via `<MC_VERIFY_URL>/api/ban`). Dismiss closes
the alert without further action. Either way the embed is
rewritten with a footer recording who took the action ("Banned
by alice" or "Dismissed by bob") and the buttons are removed so
the alert can't be acted on twice.

## Activation

Chargeback alerts are part of the Minecraft module. You need:

```toml
[features]
minecraft = true

[minecraft]
chargeback = true

[minecraft.chargeback_config]
staff_channel   = "123456789012345678"
restricted_role = "234567890123456789"
```

Plus the standard MC env vars in `.env`:

```
MC_VERIFY_URL=https://your-mc-server.example.com
MC_VERIFY_SECRET=long-random-string-shared-with-the-mc-plugin
```

The chargeback **listener** is mounted onto the bot's existing
HTTP server (the one the MCP server uses) at
`POST /webhook/chargeback`. There's no separate port; the bot's
HTTP listener handles both. See [MCP Server](mcp-server.md) for
how to expose the HTTP listener publicly.

The webhook router only spins up if all three preconditions hold:
the chargeback feature is enabled, the config sub-section is
present, and `MC_VERIFY_URL` + `MC_VERIFY_SECRET` are both set.
Missing any one of those quietly disables the route — the bot
starts cleanly, but no chargeback alerts will ever fire.

## Configuration

| Field | Type | Required | Description |
|---|---|---|---|
| `staff_channel` | string (snowflake) | yes | Channel ID where alerts are posted. |
| `restricted_role` | string (snowflake) | yes | Role ID applied to the offender (and used as the only remaining role on their account after roles are stripped). |

Both must be valid snowflakes. Invalid values cause the webhook
to return `500 Internal Server Error` and log the bad config; the
underlying chargeback isn't lost on the MC side, but no alert
gets posted on the Discord side until you fix the config.

The restricted role's purpose is twofold:

- It marks the user as "currently in a chargeback hold" so
  [donator sync](minecraft-donator-sync.md) doesn't re-grant
  their donor perks on the next tick.
- Combined with channel-level permissions on your server, it
  isolates the user from sensitive channels until staff
  resolves the alert.

Set the role's permissions and channel overrides to whatever
"banned but not yet acted on" means for your server.

## The webhook payload

The MC plugin POSTs JSON in this shape:

```json
{
  "uuid":       "069a79f4-44e9-4726-a5be-fca90e38aaf5",
  "username":   "Steve",
  "discord_id": "987654321098765432",
  "tier":       "supporter",
  "timestamp":  "2026-04-15T18:00:00Z"
}
```

`discord_id` is optional — if the player never verified, it's
`null` and the bot only takes MC-side action via the staff button.
The wire format lives in
`src/minecraft/chargeback.rs::ChargebackPayload`. Authentication
is the shared `MC_VERIFY_SECRET` sent as `Authorization: Bearer
<secret>`; treat the secret like a credential.

## The staff alert

The alert embed has a red border and the title
`⚠️ CHARGEBACK ALERT`, with fields for **Player**, **Tier**,
**Discord** (`<@id> (id)` or `Not linked`), **MC UUID**, and
**Time**. The footer summarizes the automatic action: `All roles
stripped. User restricted.` (linked) or `No Discord account
linked. MC-side actions only.` (unlinked).

Two buttons sit beneath: 🔨 **Ban** (or **Ban MC** if unlinked)
and ❌ **Dismiss**. Only members with one of three hard-coded
staff roles (see *Staff role gating* below) can press either —
anyone else gets `You don't have permission to do this.`

After a button is pressed the embed is rebuilt with a neutral
border, the footer is replaced with `Banned by <staff>` or
`Dismissed by <staff>`, and the buttons are removed.

## Ban action

When a staff member clicks **Ban**:

1. **Discord side.** If the embed shows a linked `discord_id`,
   the bot extracts it and calls `ban_user` with audit-log
   reason `Chargeback ban by <staff>`. Failures are logged but
   don't block MC side.
2. **MC side.** The bot POSTs to `<MC_VERIFY_URL>/api/ban` with
   the player's UUID and a reason identifying the staff member.
3. **Failure surfacing.** If the MC ban returns non-2xx or
   transport-fails, the bot reposts the failure into the staff
   channel: `⚠️ MC ban failed for UUID <uuid>: <status>
   <body>`.

## Dismiss action

Clicking **Dismiss** doesn't restore anything — the roles are
already stripped and the restricted role applied. It just closes
the alert with a footer recording who dismissed it. To restore a
user, staff manually removes the restricted role and re-adds
whatever roles they had before; stripped roles aren't preserved.

## Staff role gating

Button permission checks reference a hard-coded list of staff
role IDs in `src/minecraft/chargeback.rs::handle_button` —
Moderator, Admin, and Owner snowflakes baked into the source.
These IDs are **specific to the original deployment**; on your
own instance, edit them to match your staff roles and rebuild.
A future revision will likely move them into
`[minecraft.chargeback_config]`.

Until you update them, the Ban/Dismiss buttons are unusable —
the alerts still post and the auto-strip still happens, but
nobody can confirm via button. As a workaround, use `!m ban`
and let MC handle its own side.

## Permissions

The bot's role must:

- Be **higher** than the restricted role and any roles it might
  need to strip on offenders, in the role hierarchy.
- Have `MANAGE_ROLES` (to apply the restricted role and strip
  roles) and `BAN_MEMBERS` (for the Ban button).
- Have `SEND_MESSAGES` and `EMBED_LINKS` in the configured
  `staff_channel`.

Without `BAN_MEMBERS` the auto-strip will succeed but the Ban
button will fail; the alert will still post and dismiss.

## Common issues

- **No alert appears after a chargeback** — check the bot logs
  for `Chargeback webhook received: …`. If absent, the MC plugin
  isn't POSTing to the right URL or the HTTP listener isn't
  exposed. If present but no embed posts, check `staff_channel`
  and bot permissions there.
- **`401 Unauthorized` on the webhook** — `MC_VERIFY_SECRET`
  doesn't match between bot and plugin.
- **Roles aren't stripped on the offender** — only happens when
  the payload includes a linked `discord_id`. Unverified users
  get no Discord-side action; the alert footer says so.
- **Ban button does nothing for non-staff** — staff role gate.
  Update the hard-coded role IDs in `src/minecraft/chargeback.rs`
  and rebuild.
- **MC ban failed** — the `/api/ban` endpoint returned an error.
  The bot reposts the failure into the staff channel; check the
  response body.
- **Dismiss didn't restore the user's roles** — by design.
  Restoring stripped roles is a manual operation.

## Cross-references

- [Minecraft: Verify](minecraft-verify.md) — for the Discord ↔
  UUID mapping that determines whether chargeback can act
  Discord-side.
- [Minecraft: Donator Sync](minecraft-donator-sync.md) — uses
  the restricted role as a "do not re-grant donor perks"
  signal.
- [MCP Server](mcp-server.md) — how the bot's HTTP listener
  is hosted; the chargeback webhook lives on the same router.
- [Instance Config: `[minecraft.chargeback_config]`](../configuration/instance-config.md#minecraftchargeback_config-section) —
  schema reference.
- [Environment Variables](../configuration/environment-variables.md) —
  `MC_VERIFY_URL`, `MC_VERIFY_SECRET`.
