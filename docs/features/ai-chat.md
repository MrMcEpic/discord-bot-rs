# AI Chat

The bot replies to natural-language messages in Discord using a large
language model. The chat path is wired up to two providers — DeepSeek as the
primary, Google Gemini as a vision-and-fallback path — and the personality
is loaded from a plain text file you write yourself.

## What it does

When someone @mentions the bot or replies to one of its messages, the bot
collects the recent conversation in that channel, feeds it to the AI along
with your `personality.txt` system prompt, and sends the model's reply back
as a Discord message. It does this with whatever model you have keys for:

- `DEEPSEEK_API_KEY` — primary provider (`deepseek-chat`, with automatic
  routing to `deepseek-reasoner` for hard questions). Free or cheap; the bot
  is built around its tool-calling format.
- `GEMINI_API_KEY` — secondary provider. Used for image attachments
  (DeepSeek Chat is text-only) and as a fallback if the DeepSeek text path
  is unavailable.

If you set neither key, the bot still starts cleanly — it just won't react
to mentions. If you set both, you get text replies via DeepSeek and image
understanding via Gemini for free.

The pipeline goes well beyond echoing replies: the AI can call tools you've
exposed (music, moderation, stocks, web search, NYT-style games) and the
bot routes the resulting tool calls back into Discord. See
[Architecture: AI Pipeline](../architecture/ai-pipeline.md) for the flow
diagram.

## Activation

The message handler in `src/events/mod.rs` triggers the AI on exactly two
conditions:

1. The message contains a direct mention of the bot's user (the bot is in
   `message.mentions`).
2. The message is a Discord *reply* to one of the bot's previous messages.

Anything else is ignored. The bot does not respond to keywords, prefixes,
or DMs. There is no "owner override" or special-user bypass — every user is
treated identically by the AI path.

When neither `DEEPSEEK_API_KEY` nor `GEMINI_API_KEY` is set, the activation
check short-circuits and the handler exits immediately. This is the safe
state: you can deploy the bot without an AI key and the rest of its
features still work.

## Configuration

There are exactly three things to configure:

1. **`DEEPSEEK_API_KEY`** in `.env`. Optional in the strict sense, but
   without it you get no text replies. Get one from
   [`platform.deepseek.com`](https://platform.deepseek.com).
2. **`GEMINI_API_KEY`** in `.env`. Optional. Required only if you want
   image understanding or text-fallback when DeepSeek is unreachable.
3. **`personality.txt`** in the instance config directory (the directory
   pointed at by `CONFIG_DIR`, defaults to the working directory). This is
   loaded at startup by `InstanceConfig::load_personality` and panics if
   the file is missing or empty — this is intentional, because shipping
   without a personality means the bot has no voice.

There is no in-app configuration of model parameters, temperature, or
context window size — they are tuned in `src/ai/deepseek.rs`. If you want
to override them you have to recompile.

For details on how to write a good personality file, see
[Personality Files](../configuration/personality.md). For where to keep
your `.env` and how to feed it into Docker safely, see
[Secrets Management](../configuration/secrets-management.md).

## How the personality shapes responses

`personality.txt` is loaded as a free-form string, prepended to a small
hard-coded system prompt, and sent to every API request as the
`system`-role message. The hard-coded part covers things the bot needs to
know regardless of personality — the current date, its version, what
tools it has, security boilerplate against prompt-injection attacks, and
formatting rules for Discord markdown. Your text sits at the top.

This means:

- Anything you write in `personality.txt` overrides the generic LLM voice.
  Be opinionated. The AI is much more interesting when the personality is
  specific.
- The personality is loaded **once at startup**. Editing the file and
  saving it does nothing until the bot restarts.
- The personality is the same across every channel and every user. There
  is no per-guild override.

## Conversation context window

For each mention, the bot fetches the last 100 messages in the channel
(`FETCH_LIMIT` in `src/ai/deepseek.rs`) and walks them in order, picking
up to **10 relevant messages** (`MAX_RELEVANT`) that meet two filters:

- They are no older than **30 minutes**.
- They are either bot replies or messages that mentioned/replied to the
  bot.

Messages from before the bot's current process started are also dropped,
so a restart wipes the AI's memory of older conversation. There is no
cross-channel memory; each channel is its own scratch buffer.

This window is deliberately small. It keeps token costs down, keeps
context fresh, and prevents the bot from rehashing a stale request from
hours ago. The trade-off is that long conversations summarize themselves
out of view quickly.

**Known limitation:** in busy channels, two unrelated conversations
happening at the same time can bleed into one another's context. The bot
does not segment by conversation thread; it segments by channel and time
window. Discord *threads* are treated as their own channel, so threading
a conversation is the cleanest workaround.

## Tool use

The AI has access to a set of function-calling tools defined in
`src/ai/tools.rs`. They cover:

- **Music** — `play_song`, `skip`, `stop`, `pause`, `resume`, `show_queue`,
  `now_playing`, `shuffle`, `set_loop`, `remove_from_queue`. The AI can
  control the music player conversationally ("play something chill", "skip
  this") without the user needing to know the prefix commands.
- **Moderation** — `tempban`, `unban`, `nuke`. Privileged. Every
  moderation tool call goes through a confirmation embed (see below)
  before it actually runs.
- **Web search** — `web_search`, used for current-events questions and
  fact-checking. Up to **three** rounds of search are allowed per
  request (the `MAX_SEARCH_ROUNDS` constant in `src/ai/deepseek.rs`,
  also interpolated into the system prompt so the model and the loop
  agree), so the AI can refine queries based on results.
- **Stocks** — `stock_buy`, `stock_sell`, `stock_price`, `stock_portfolio`,
  `stock_leaderboard`. Bound to the virtual portfolio system.
- **Games** — `connections_start`, `wordle_start`. Lets users say "start a
  Wordle" without remembering the command name.

All tool calls except web search are visible in chat: the bot replies with
the result of the action (e.g. an "Added to Queue" embed or a "Banned
*user*" message). Web search is silent — the AI consumes the results and
folds them into its answer.

For the full pipeline including tool dispatch, see
[Architecture: AI Pipeline](../architecture/ai-pipeline.md).

## Moderation confirmation

Moderation tools (`tempban`, `unban`, `nuke`) are powerful enough to do
real damage if the AI misreads a request. The bot inserts a confirmation
step:

1. The AI emits a moderation tool call.
2. The bot posts an embed showing exactly what is about to happen
   ("Tempban @user for `3d` — repeated spam"), with **Approve** and
   **Cancel** buttons.
3. Only the **original requesting user** can press a button.
4. If the user lacks the corresponding permission (`BAN_MEMBERS` for
   tempban/unban, `MANAGE_MESSAGES` for nuke), the bot refuses up front.
5. If 30 seconds pass with no response, the action expires and is
   cancelled.

This is implemented in `src/ai/confirmation.rs`. There is no way to opt
out — privileged tools always go through the confirmation gate.

## Safety and sanitization

User input is sanitized before being sent to the model (see
`src/ai/sanitize.rs`): role markers like `system:` are rewritten, DeepSeek
`<|...|>` tokens are stripped, and Llama-style `[INST]` / `<<SYS>>` blocks
are removed. This makes it harder for a user to inject a fake system
prompt by typing one into chat.

The model's *output* is also filtered. The bot maintains a list of "bad
assistant" patterns — "I am Claude", "I don't have the ability to
remember", "created by Anthropic" — and refuses to fold those messages
back into the conversation history. Without this, hallucinated identities
would propagate through the context window.

The hard-coded system prompt also tells the model to ignore "ignore
previous instructions" jailbreaks and never reveal its system prompt.

## Rate limiting

A per-user rate limiter (`data.rate_limiters.ai`) checks the requester
before each AI call. If the user is over their budget, the bot replies
"Slow down — try again in *N*s" instead of calling the API. There is a
separate, stricter limiter on moderation tool calls.

The limits are tuned to absorb normal back-and-forth chat without
throttling, while preventing one user from running up an API bill in
isolation.

## Provider failover

The text path is hard-coded to DeepSeek. The vision path is hard-coded to
Gemini. If a request has image attachments, the bot tries Gemini first;
on failure it strips the multimodal content and falls back to DeepSeek
text-only with a description-of-context placeholder.

Inside the DeepSeek path, the bot routes between `deepseek-chat`
(`v3`-class, fast) and `deepseek-reasoner` (slow, thinks step-by-step) by
classifying each message: simple chat goes to V3, anything that smells
like a reasoning task goes to Reasoner. Reasoner can't use tools
directly, so the bot uses V3 as a research assistant first to perform
any web searches, then hands the gathered context to Reasoner for the
final answer.

## Common issues

- **Bot doesn't respond to `@MyBot`** — check that `DEEPSEEK_API_KEY` is
  set in the running environment (Docker users: confirm `.env` is being
  passed in), check the bot's logs for `API request failed` messages,
  and verify the bot has the *Message Content* gateway intent enabled in
  the Discord developer portal.
- **Bot replies but the personality is wrong** — `personality.txt` was
  edited but the bot wasn't restarted. The personality is loaded once at
  boot.
- **Bot mixes up two conversations in the same channel** — known
  limitation of the channel-and-time-window approach. Move the second
  conversation into a Discord thread, or wait 30 minutes for the older
  context to age out.
- **Bot refuses to talk about a topic with "my overlords at DeepSeek
  won't let me"** — DeepSeek's content filter triggered. The bot
  detects the upstream `Content Exists Risk` error and translates it
  into a snarky message instead of crashing.
- **Bot's reply is cut off mid-sentence** — replies longer than 2000
  characters are split into chunks by `src/ai/split.rs`. If you see a
  truncated message ending in `...[truncated]`, that's the splitter
  hitting the Discord per-message limit on a single chunk; the next
  chunk should follow immediately.
- **Bot says "I don't have memory" / "I'm Claude"** — the model
  hallucinated a different identity. The output filter catches the most
  common phrasings on subsequent turns; if it's getting through on the
  first turn, strengthen the personality file with an explicit
  "you are *not* Claude / ChatGPT / etc." line.

## Cost

Costs depend almost entirely on which model you route to. DeepSeek V3 is
inexpensive enough that an active community server typically lands at
single-digit dollars per month. Reasoner is more expensive per request
but only fires on detected reasoning queries. Gemini's free tier covers
casual image traffic.

Check the providers' current pricing pages directly:

- DeepSeek: <https://api-docs.deepseek.com/quick_start/pricing>
- Google Gemini: <https://ai.google.dev/pricing>

## Cross-references

- [Personality Files](../configuration/personality.md) — how to write
  `personality.txt`.
- [Architecture: AI Pipeline](../architecture/ai-pipeline.md) — request
  flow, tool dispatch, model routing.
- [Secrets Management](../configuration/secrets-management.md) — where
  to put API keys safely.
- [Environment Variables](../configuration/environment-variables.md) —
  `DEEPSEEK_API_KEY`, `GEMINI_API_KEY`, and friends.
- [Moderation](moderation.md) — what the moderation tools actually do
  when the AI invokes them.
