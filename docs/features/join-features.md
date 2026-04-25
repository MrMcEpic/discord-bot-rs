# Member Join Features

Two independent features that fire when a new member joins a guild: an
automatic role assignment ("join role"), and an AI-generated welcome
message ("welcome"). Either can be enabled without the other; they're
configured in separate sections of `config.toml`.

Both react to the same Discord event (`GuildMemberAddition`) and run
sequentially in the bot's `member_join` handler. If you have both
enabled, the join-role assignment happens first, then the welcome
message is generated and posted.

## Join role

The simplest behaviour the bot ships with: when a new member joins,
add a single role to them.

The most common use is marking unverified accounts so they can't see
sensitive channels until they go through whatever onboarding flow you
have set up. Another common use is granting a default colour role so
new members aren't completely roleless on arrival.

### Activation

Enable in `config.toml`:

```toml
[features]
join_role = true

[join_role]
role = "123456789012345678"   # the role ID to assign
```

Restart the bot. The startup log will show:

```
Join-role module enabled (role=123456789012345678)
```

If you set `features.join_role = true` but omit the `[join_role]`
section, the bot logs a warning and the feature stays off rather
than refusing to boot.

### Configuration

| Field | Type | Required | Description |
|---|---|---|---|
| `role` | string (snowflake) | yes | The role ID to add to every new member. |

That's it. There is no per-guild override (it applies to every guild
the bot instance serves), no exclusion list, and no condition. If a
member joins, they get the role.

### How it works

`src/events/member_join.rs::handle_member_join` is invoked by the
event dispatcher every time a `GuildMemberAddition` event fires.
The first thing it does is check `data.join_role_config`. If
present, it parses the role ID and calls `add_member_role` with
the audit-log reason `Auto join role`. Failures are logged but
otherwise swallowed — the bot doesn't try to retry.

### Permissions

The bot's role must be **above** the configured role in the role
hierarchy and have `MANAGE_ROLES`. Without that, the assignment
will fail with a Discord permission error and the member will be
left without the role. Watch the logs for
`Failed to assign join role to <user>: …`.

### Common issues

- **Role isn't being assigned** — bot's role lacks `MANAGE_ROLES`
  or sits below the target role in the hierarchy.
- **Members joined while the bot was offline** — there's no
  catch-up. The handler only fires for live `GuildMemberAddition`
  events. Existing roleless members need to be batched manually.
- **`Invalid join role ID` warning** — the configured `role` value
  isn't a numeric snowflake. Make sure Developer Mode is on and
  you copied the role's ID, not its name.

## Welcome messages

When a new member joins, the bot can post an AI-generated welcome
message to a designated channel. The message uses your bot's
personality plus an additional welcome-specific prompt to produce
something on-brand and member-specific (mentions the new user,
ideally riffs on their display name).

### Activation

Welcome messages have three prerequisites:

1. `features.welcome = true` in `config.toml`.
2. A `[welcome]` section pointing at the channel and an optional
   custom prompt file.
3. At least one AI provider key — `DEEPSEEK_API_KEY` or
   `GEMINI_API_KEY` — in `.env`.

If you set the flag but skip the prompt file, the welcome stays off
silently (logged at warn level). Same if neither AI key is set.

```toml
[features]
welcome = true

[welcome]
channel     = "123456789012345678"          # where to post the welcome
prompt_file = "welcome_prompt.txt"          # default; relative to config dir
```

Then create `welcome_prompt.txt` next to `config.toml`. This is
**not** the bot's overall personality — that's `personality.txt`,
loaded for AI chat. The welcome prompt is appended to the
personality and tells the model specifically how to write a
welcome.

A short welcome prompt might look like:

```
Write a one-paragraph welcome message for the new member.
Keep it under 80 words, be warm but on-brand, and mention them
by their Discord display name (already provided in the user
message). Suggest one channel they should check out first.
```

### How it works

After the join-role step, `handle_member_join` checks for both
`welcome_config` and `welcome_prompt`. If both are present, it:

1. **Rate-limits per user.** Each joining user gets their own
   5-second budget through the shared `RateLimiters` infrastructure
   (1 request / 5 seconds, keyed on the joining user's ID). A
   single user re-joining repeatedly is throttled, but a raid of
   distinct users all get their own welcomes.
2. **Picks a provider.** If `DEEPSEEK_API_KEY` is set, use
   DeepSeek (`deepseek-v4-flash`). Otherwise fall back to Gemini
   (`gemini-3-flash-preview`). Both providers are called via
   their OpenAI-compatible chat endpoints.
3. **Builds the system prompt.** Concatenates the bot's full
   personality file with the welcome prompt under a
   `## Welcome Message Instructions` heading.
4. **Sends one user message.** It tells the model the new
   member's display name and mention string and asks it to
   write a welcome.
5. **Posts the model's response.** The reply text goes straight
   to the configured channel as a regular message (no embed).

The request has a 30-second timeout and a 512-token max. Failures
(API down, model returns empty content, channel doesn't exist,
bot lacks permission) are logged at `warn` and silently swallowed
— no fallback "Welcome!" message is sent.

### Configuration

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `channel` | string (snowflake) | yes | — | Channel ID to post the welcome in. |
| `prompt_file` | string | no | `"welcome_prompt.txt"` | Path (relative to the config directory) to the welcome prompt. |

### Rate limiting

Welcome generation is rate-limited **per joining user** through the
bot's shared `RateLimiters` infrastructure: 1 request per 5 seconds,
keyed on the new member's user ID. The same user re-joining within
the window gets a single welcome; ten distinct users joining inside
the same five seconds get ten welcomes (one each).

This is the fix for the older global-mutex rate limit, which
suppressed legitimate joins during a raid: a burst of 10 joiners
used to produce one welcome and nine silent skips. Now the
limiter only throttles a single user's repeated joins, not the
whole channel.

### Permissions

The bot needs `SEND_MESSAGES` and the standard message-content
permissions in the welcome channel. It does **not** need
`EMBED_LINKS` since welcomes are plain text. If permissions are
missing, the bot logs `Failed to send welcome message: …` and
moves on.

### Cost

One AI call per join, capped at 512 output tokens. On DeepSeek V4-Flash
that's a fraction of a cent per welcome — even an active server
won't notice the bill. If you're using Gemini, the free tier
covers most casual join volumes; check
<https://ai.google.dev/pricing> for current rates.

### Common issues

- **No welcome appears** — check the logs at startup. If you don't
  see `Welcome module enabled`, either `features.welcome = false`,
  the `[welcome]` section is missing, or the prompt file is
  missing/empty. If you do see the enabled line but no AI key
  warning, verify `DEEPSEEK_API_KEY` or `GEMINI_API_KEY` is in
  the running environment.
- **Welcome appears but the personality is wrong** — the
  personality file is loaded once at boot. Restart the bot after
  editing `personality.txt` or `welcome_prompt.txt`.
- **Welcome is generic / boring** — the welcome prompt is
  inheriting the bot's overall personality. Make the prompt more
  specific — tell it the server's tone, what to highlight, what
  to avoid.
- **The same user keeps re-joining and only gets one welcome** —
  the per-user 5-second rate limiter. Distinct users in a raid
  each get their own welcome; a single user looping through
  join/leave/join is throttled.
- **Welcome posted to wrong channel** — verify the snowflake ID
  in `[welcome].channel` is for the channel you actually meant.
  No mention syntax; just the raw ID as a string.

## Cross-references

- [Instance Config: `[join_role]`](../configuration/instance-config.md#join_role-section)
  and [`[welcome]`](../configuration/instance-config.md#welcome-section) — schema references.
- [AI Chat](ai-chat.md) — how the AI provider stack works; welcomes
  reuse it.
- [Personality Files](../configuration/personality.md) — where
  `personality.txt` and `welcome_prompt.txt` are sourced.
- [Auto-Role](auto-role.md) — for the post-join "graduate the
  member" promotion flow.
- [Secrets Management](../configuration/secrets-management.md) —
  where to put `DEEPSEEK_API_KEY` and `GEMINI_API_KEY` safely.
