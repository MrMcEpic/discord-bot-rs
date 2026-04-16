# Connections

The New York Times' "find the four hidden groups" word puzzle, played
inside Discord with button controls. The bot fetches the official puzzle
from the NYT API and renders the 16-word board as a grid of clickable
buttons.

## What it does

A Connections puzzle gives you sixteen words. Those words sort into four
hidden categories of four words each, each category colour-coded by
difficulty: yellow (easiest), green, blue, and purple (hardest). Your
job is to identify all four groups. You're allowed four mistakes before
the game ends.

The bot:

- Fetches the puzzle for a given date from the NYT API
  (`nytimes.com/svc/connections/v2/<date>.json`).
- Shuffles the 16 words and renders them as four rows of four buttons.
- Lets users click words to select them, click again to deselect, and
  hit **Submit** when they have exactly four selected.
- Tracks selected words, solved categories, mistakes remaining, and a
  status line showing the result of the last guess.
- Detects "one away" guesses (three of four correct) and tells you so,
  the same way the official NYT version does.
- Reveals all four categories when the game ends, win or lose.

Three flavours of puzzle are available, mirroring the Wordle commands:
today's daily puzzle, a specific date, or a random one from the
back catalogue.

Connections is always-on. There's no `[features]` flag and no API key
required.

## Commands

All commands live under the `m` parent command. With the default `!`
prefix that means **`!m connections <subcommand>`**. The prefix is
configurable per instance.

| Command | Aliases | Description |
|---|---|---|
| `!m connections` | `!m conn` | Start today's puzzle in this channel. |
| `!m connections random` | `!m conn rand`, `!m conn r` | Start a random puzzle from the back catalogue. |
| `!m connections date <YYYY-MM-DD>` | `!m conn d <date>` | Start a specific date's puzzle. |

Starting a new game in a channel that already has one in progress
**replaces** the old game. One puzzle per channel at a time.

## How to play

1. Run `!m connections` (or one of the variants above). The bot posts
   an embed with the title `Connections — YYYY-MM-DD`, a "Mistakes
   remaining: ⬛⬛⬛⬛" line, and four rows of word buttons.
2. Click words to select them. Selected words flip from grey
   (`Secondary`) to blue (`Primary`). The Submit button only enables
   once you have exactly four selected.
3. Click **Deselect** to clear your current selection, **Shuffle** to
   randomize the remaining words on the board, or **Submit** to
   commit the four-word guess.
4. Each guess produces one of three outcomes:
   - **Correct** — the four words form one of the hidden categories.
     The bot adds them to the solved list at the top of the embed
     (annotated with the category title and difficulty colour) and
     removes them from the board.
   - **One away** — three of your four words are in the same hidden
     category but the fourth doesn't fit. The bot says "One away"
     and decrements your mistakes counter.
   - **Wrong** — none of the unsolved categories matches three or
     more of your words. Counter decrements.
5. Continue until you solve all four groups (win) or run out of
   mistakes (lose). At game end the bot reveals every group with
   its words and category title.

The status line under "Mistakes remaining" shows the most recent
result and which user made the guess: e.g. `🟪 Tricky Wordplay solved
by @alice!` or `❌ One away! (guessed by @bob) — 2 mistakes
remaining`.

Anyone in the channel can press the buttons. Connections is
collaborative by default — the bot doesn't restrict guessing to a
single user.

## The board representation

The bot embed has three parts:

1. **Solved groups** at the top, in difficulty order (yellow → purple),
   each line showing the colour emoji, the category title, and the
   four words.
2. **Mistakes remaining** as a row of four squares: `⬛` for each
   remaining mistake, `✖️` for each used.
3. **Status message** showing the result of the last action.

Below the embed, an action row holds the word buttons (4 per row, up to
4 rows for 16 words; the row count shrinks as you solve groups) plus
a control row with **Shuffle**, **Deselect**, and **Submit**. When
the game ends, all buttons disappear.

## Game lifecycle

Connections game state lives in an in-memory `DashMap` keyed by Discord
channel ID. Two things end a game:

- **Completion.** A win or a fourth mistake. The bot rewrites the embed
  with the answer key and removes the buttons.
- **Inactivity.** A game that hasn't received a button press in
  **30 minutes** is treated as expired. The next click after expiry
  gets an ephemeral "Game expired due to inactivity" message and the
  game is dropped from memory.

Restarting the bot wipes all in-progress games. Nothing about
Connections touches the database; there is no persistence and no
leaderboard.

## Configuration

There is nothing to configure. Connections ships always-on and uses no
environment variables. The puzzle archive starts at **2023-06-12**
(the first NYT Connections puzzle). Earlier dates return "No puzzle
found for date".

## Common issues

- **"No puzzle found for date `YYYY-MM-DD`"** — either the date is
  before `2023-06-12` or the date is malformed. Use ISO format,
  zero-padded.
- **"Select exactly 4 words before submitting"** — the Submit button
  is meant to be disabled until you have four selected, but if you
  press it via tooling that ignores the disabled state, you'll get
  this ephemeral notice.
- **"No active game in this channel"** when pressing a word button —
  the previous game expired or was replaced. Start a new one.
- **The board doesn't shrink when I solve a group** — it does, but
  Discord caches embeds aggressively in some clients. Refresh the
  channel.
- **Two people clicking words at the same time fight each other** —
  the bot serializes each click through a per-game lock, so there's
  no race, but you may briefly see a button toggle on/off if two
  users click the same word in quick succession.
- **Today's puzzle isn't available yet** — the NYT publishes the new
  puzzle around midnight Eastern. Until then the bot's "today" date
  (UTC) may resolve to a not-yet-available puzzle and you'll get the
  "No puzzle found" error. Try `!m connections date <yesterday>`.

## Cross-references

- [Wordle](games-wordle.md) — sister command, same daily-NYT pattern.
- [AI Chat](ai-chat.md#tool-use) — the AI can invoke
  `connections_start` as a tool, so users can say "@bot start
  Connections" without remembering the command.
- [Instance Config](../configuration/instance-config.md) — for
  `command_prefix`.
