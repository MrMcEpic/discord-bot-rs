# Command List

Every bot command supported by discord-bot-rs, grouped by source module.

All user-facing commands are **prefix subcommands** of a single parent command, `m`. The default prefix is `!` (configurable per instance via `command_prefix` in `config.toml`), so you invoke them as `!m <subcommand> [args]`. None of the public commands are registered as slash commands — discord-bot-rs is a prefix-first bot. The parent `m` command itself does nothing on its own; it just routes to the subcommands listed below.

The single command registered with the poise framework lives in [`src/commands/mod.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/commands/mod.rs) and is built in [`src/main.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/main.rs). The `verify` subcommand is added dynamically when the Minecraft verification module is enabled in `config.toml`.

## Music

Defined in `src/commands/music.rs`. All music commands respect DJ mode: when DJ mode is enabled (`!m djmode`), only members with the configured DJ role (or administrators) may run them.

- `!m play <query>` — Play a song. Joins your current voice channel and either starts playback or appends to the queue.
  - `query` (string, required, rest) — Song name or URL (YouTube, etc.).
  - Aliases: `p`.
- `!m playlist <url>` — Queue an entire playlist at once.
  - `url` (string, required, rest) — Playlist URL.
  - Aliases: `pl`.
- `!m skip` — Skip the current track and advance to the next item in the queue. Leaves voice if the queue becomes empty.
  - Aliases: `s`.
- `!m stop` — Stop playback, clear the queue, and leave the voice channel.
- `!m pause` — Pause the current track.
- `!m resume` — Resume a paused track.
  - Aliases: `r`.
- `!m queue` — Show the current queue as an embed.
  - Aliases: `q`.
- `!m nowplaying` — Show what is currently playing, with playback controls.
  - Aliases: `np`.
- `!m remove <position>` — Remove a single track from the queue by 1-based position.
  - `position` (integer, required) — Queue position to remove.
- `!m loop [mode]` — Toggle or set loop mode. With no argument, cycles through `off` → `track` → `queue`.
  - `mode` (string, optional) — One of `off`/`none`, `track`/`t`, or `queue`/`q`.
  - Aliases: `l`.
- `!m shuffle` — Shuffle the queued tracks (does not affect the currently playing track).

## Moderation

Defined in `src/commands/moderation.rs`. All moderation commands write to the audit log channel set by `!m setlog` (if any).

- `!m ban <user> <duration> [reason]` — Temporarily ban a user. The bot stores the expiration in the database and a background task auto-unbans when it elapses.
  - `user` (member mention/ID, required) — Member to ban.
  - `duration` (string, required) — Duration like `30s`, `5m`, `2h`, `3d`, `1w`.
  - `reason` (string, optional, rest) — Audit-log reason.
  - Required permissions: `BAN_MEMBERS`.
- `!m unban <user>` — Unban a user early. Clears any matching tempban record.
  - `user` (user mention/ID, required) — User to unban.
  - Required permissions: `BAN_MEMBERS`.
- `!m banlist` — Show all currently active tempbans for this guild.
  - Aliases: `bans`.
  - Required permissions: `BAN_MEMBERS`.
- `!m nuke <count>` — Bulk-delete the most recent messages in the current channel. Discord's bulk-delete API rejects messages older than 14 days.
  - `count` (integer 1–100, required) — Number of messages to delete.
  - Required permissions: `MANAGE_MESSAGES`.

## Admin

Defined in `src/commands/admin.rs`. These configure per-guild settings stored in the database.

- `!m setlog <channel>` — Set the audit-log channel where moderation actions are reported.
  - `channel` (channel mention/ID, required) — Target channel.
  - Required permissions: `ADMINISTRATOR`.
- `!m djrole <role>` — Set the DJ role used by DJ mode.
  - `role` (role mention/ID, required) — Role to mark as DJ.
  - Required permissions: `ADMINISTRATOR`.
- `!m djmode` — Toggle DJ-only mode. When enabled, music commands require either administrator permission or the configured DJ role. Refuses to enable until a DJ role is set.
  - Required permissions: `ADMINISTRATOR`.

## Stocks

Defined in `src/commands/stocks.rs`. The `stock` parent command is itself a subcommand group with its own subcommands. All stock commands require `FINNHUB_API_KEY` to be configured.

- `!m stock` — Bare invocation shows your current portfolio.
  - Aliases: `stocks`, `st`.
- `!m stock buy <symbol> <amount>` — Buy shares. The amount may be a share count (`5`) or a dollar amount (`$500`).
  - `symbol` (string, required) — Ticker symbol.
  - `amount` (string, required, rest) — Quantity or `$amount`.
  - Aliases: `b`.
- `!m stock sell <symbol> <amount>` — Sell shares. The amount may be a quantity or `all`.
  - `symbol` (string, required) — Ticker symbol.
  - `amount` (string, required, rest) — Quantity or `all`.
  - Aliases: `s`.
- `!m stock portfolio [user]` — View a portfolio (yours by default).
  - `user` (user mention/ID, optional) — Other user to inspect.
  - Aliases: `port`, `pf`, `p`.
- `!m stock price <symbol>` — Show the current quote for a stock.
  - `symbol` (string, required, rest) — Ticker symbol.
  - Aliases: `quote`, `q`.
- `!m stock leaderboard` — Top 10 portfolios in this server, ranked by total value (cash + holdings).
  - Aliases: `lb`, `top`.
- `!m stock history` — Show the user's 10 most recent trades.
  - Aliases: `hist`, `h`.
- `!m stock reset [confirm]` — Reset the user's portfolio back to $1,000 cash and wipe holdings/history. Without `confirm`, prints a confirmation prompt.
  - `confirmation` (string, optional, rest) — Type `confirm` to actually reset.

## Games: Connections

Defined in `src/commands/connections.rs`. NYT Connections puzzles.

- `!m connections` — Start today's NYT Connections puzzle in this channel. Replaces any existing game in the channel.
  - Aliases: `conn`.
- `!m connections random` — Start a random Connections puzzle.
  - Aliases: `rand`, `r`.
- `!m connections date <YYYY-MM-DD>` — Start the Connections puzzle from a specific date.
  - `date` (string, required, rest) — Date in `YYYY-MM-DD` format.
  - Aliases: `d`.

## Games: Wordle

Defined in `src/commands/wordle.rs`. NYT Wordle puzzles.

- `!m wordle` — Start today's Wordle puzzle in this channel.
  - Aliases: `w`.
- `!m wordle random` — Start a random Wordle.
  - Aliases: `rand`, `r`.
- `!m wordle date <YYYY-MM-DD>` — Start the Wordle from a specific date.
  - `date` (string, required, rest) — Date in `YYYY-MM-DD` format.
  - Aliases: `d`.

## Minecraft

Defined in `src/commands/minecraft.rs`. The `verify` subcommand is only registered when `[features].minecraft = true` **and** `minecraft.verify = true` in `config.toml`. It also requires `MC_VERIFY_URL` and `MC_VERIFY_SECRET` to be set.

- `!m verify <code>` — Link your Discord account to a Minecraft username using a code generated by `/verify` in-game.
  - `code` (string, required, rest) — Verification code from Minecraft.

## Help

Defined in `src/commands/help.rs`.

- `!m help` — Show the embedded help message. Sections shown depend on the caller's permissions: moderation/admin sections only appear for users with the relevant permissions.
  - Aliases: `h`.
