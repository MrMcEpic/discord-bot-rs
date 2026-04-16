# Wordle

A Discord-native port of the New York Times' daily five-letter word puzzle.
The bot fetches the official puzzle from the NYT API and tracks each game
in memory, scoped to a single channel.

## What it does

Wordle is the classic six-guess word game: you have six attempts to guess a
five-letter solution. Each guess is graded letter-by-letter — green for the
right letter in the right spot, yellow for the right letter in the wrong
spot, black for a letter that isn't in the solution at all. The bot owns
the score grid and the "keyboard" tracker; you just type your guesses into
chat as plain messages.

Three flavours of puzzle are available:

- **Today's puzzle** — the same one everybody else is playing. Pulled from
  the NYT API in UTC.
- **A specific date** — replay any puzzle from `2021-06-19` (the first NYT
  Wordle) onwards.
- **A random puzzle** — pick any past puzzle uniformly at random from the
  full back catalogue.

Wordle is always-on. There is no `[features]` flag and no API key required.
The bot calls a public NYT JSON endpoint
(`nytimes.com/svc/wordle/v2/<date>.json`) and the word list of valid guesses
is bundled into the binary (`src/wordle/words.txt`).

## Commands

All commands live under the `m` parent command. With the default `!`
prefix that means **`!m wordle <subcommand>`**. The prefix is configurable
per instance via `command_prefix` in `config.toml`; the examples assume
`!`.

| Command | Aliases | Description |
|---|---|---|
| `!m wordle` | `!m w` | Start today's puzzle in this channel. |
| `!m wordle random` | `!m w rand`, `!m w r` | Start a random puzzle from the back catalogue. |
| `!m wordle date <YYYY-MM-DD>` | `!m w d <date>` | Start a specific date's puzzle. The argument is `#[rest]`, so trailing whitespace is fine. |

Starting a new game in a channel that already has one in progress
**replaces** the old game — there's no "are you sure?" prompt. One puzzle
per channel at a time.

## How to play

1. Start the puzzle with one of the commands above. The bot posts an
   embed showing six empty rows and a `Wordle — YYYY-MM-DD` title.
2. Type a five-letter word into the channel. The bot detects any message
   in the channel that's **exactly five ASCII letters** while a game is
   active — no command prefix needed.
3. The bot deletes your guess message and edits the embed in place,
   filling in the next row with green/yellow/black squares and updating
   the keyboard tracker at the bottom.
4. Repeat until you solve it or run out of guesses.

If you type a five-letter sequence that isn't a real word, the bot
replies "Not a valid word." and deletes both your message and its reply
after about two seconds. You don't lose a guess.

When you win, the title flips to `Wordle — YYYY-MM-DD — Solved in N!`
with a green border. When you lose, the title flips to `Game Over`, the
border goes red, and the answer is revealed under the grid.

## The keyboard tracker

Below the grid the bot maintains a per-letter status line that summarizes
what you've learned so far:

```
🟩 A E   🟨 R   ⬛ S T O U N D L
```

Letters get "upgraded" as you discover better information about them: a
letter that started as black (`⬛ Absent`) gets promoted to yellow
(`🟨 Present`) the first time you see it in a yellow square, and to green
(`🟩 Correct`) the first time you see it in a green one. The tracker
never downgrades a letter, so you can scan it for what's still on the
table at a glance.

## Game lifecycle

Wordle game state lives in an in-memory `DashMap` keyed by Discord channel
ID. Two things end a game:

- **Completion.** When you win or lose, the game is removed from the map
  immediately. The embed stays in the channel as a record.
- **Inactivity.** A game that hasn't received a guess in **30 minutes**
  is treated as expired. Expired games are cleaned up the next time
  someone tries to interact with the channel.

Restarting the bot wipes all in-progress games. There is no persistence;
nothing about Wordle touches the database. If you start the daily puzzle,
guess a few times, and the bot restarts, you're back to a fresh six
guesses.

There is no per-user lockout — once a game is going in a channel,
**anyone in that channel can guess**. This makes it work nicely as a
collaborative puzzle.

## Configuration

There is nothing to configure. Wordle ships always-on and uses no
environment variables. If your instance hides the games behind DJ mode
or a role gate, that's outside Wordle's scope; gate the channel itself
with Discord permissions.

## Common issues

- **"No Wordle found for date `YYYY-MM-DD`"** — either the date is
  before `2021-06-19` (when the NYT bought Wordle and started serving
  puzzles via this API) or the date string is malformed. Use ISO format,
  zero-padded.
- **Five-letter guesses do nothing** — there is no active game in the
  channel. Start one with `!m wordle`. Or the previous game just expired
  (30 minutes idle); the next guess after expiry is silently ignored
  rather than triggering anything weird.
- **Bot says "Not a valid word."** — your guess isn't in the bundled
  word list (`src/wordle/words.txt`). The list is the standard Wordle
  guess vocabulary; it does not include slang, names, or every English
  word. Try a different guess.
- **Wins/losses don't show up on a leaderboard** — there is no Wordle
  leaderboard. Game state is per-channel and ephemeral, by design.
- **Multiple people guessing at the same time** — that's fine. The
  bot serializes guesses through a per-game lock and applies them in
  the order they arrive. The grid will update once per guess.

## Cross-references

- [Connections](games-connections.md) — sister command, same daily-NYT
  pattern.
- [Stocks](games-stocks.md) — the third "always-on" game-like feature.
- [AI Chat](ai-chat.md#tool-use) — the AI can invoke `wordle_start` as
  a tool, so users can say "@bot start a Wordle" without remembering
  the command.
- [Instance Config](../configuration/instance-config.md) — `command_prefix`
  if you've changed it from the default `!`.
