# Music

A queue-based voice music player. Audio is fetched and remuxed by `yt-dlp`
into an Opus-in-OGG stream, which Discord (via `songbird`) plays back
without any additional transcoding.

## What it does

- One queue per Discord guild, up to 100 tracks (`MAX_QUEUE_LENGTH` in
  `src/music/player.rs`).
- Plays anything `yt-dlp` can resolve: YouTube videos, YouTube playlists,
  SoundCloud, Bandcamp, Mixcloud, Vimeo, direct media URLs, and several
  hundred other sites.
- Search-by-text via the `ytsearch1:` prefix when you don't have a URL.
- Loop modes (off / track / queue), shuffle, queue editing.
- An interactive "Now Playing" embed with button controls.
- Auto-leave the voice channel after 5 minutes of nothing playing.

The bot speaks 256 kbps Opus directly, so there is no transcoding step
inside the process — `yt-dlp` hands `songbird` an Opus stream and
`songbird` forwards it to Discord. CPU usage is essentially flat, even
on small VPSes.

## Commands

All music commands live under the `m` parent. With the default `!`
prefix that means **`!m <subcommand>`**. The prefix is configurable per
instance via `command_prefix` in `config.toml`; the examples below
assume `!`.

| Command | Aliases | Description |
|---|---|---|
| `!m play <url-or-query>` | `!m p` | Play immediately, or queue if something is already playing. Accepts a URL or a free-text search. The `query` argument is `#[rest]`, so spaces don't need quoting. |
| `!m playlist <playlist-url>` | `!m pl` | Resolve every track in a playlist URL and queue them all. The first one starts playing, the rest go into the queue (capped by `MAX_QUEUE_LENGTH`). |
| `!m skip` | `!m s` | Skip the current track. If there's a next track in the queue (or loop mode says to repeat), it starts immediately; otherwise the bot stops and idles. |
| `!m stop` | — | Stop playback, clear the queue, leave the voice channel. The opposite of `!m play`. |
| `!m pause` | — | Pause the current track. The voice connection stays up. |
| `!m resume` | `!m r` | Resume a paused track. |
| `!m queue` | `!m q` | Show the queue: now-playing, the next 15 tracks (with a "+ N more" line if longer), and total duration. |
| `!m nowplaying` | `!m np` | Show the current track in a fresh "Now Playing" embed with control buttons. |
| `!m remove <position>` | — | Remove a queued track by 1-based position. |
| `!m loop [off\|track\|queue]` | `!m l` | Set the loop mode. With no argument, cycles through the modes. `track` repeats the current song; `queue` re-enqueues finished tracks at the back. |
| `!m shuffle` | — | Randomize the order of the pending queue. |

There is no `previous`, no `seek`, and no playback-position scrubbing.
The model is "modify the queue, then let it play" rather than
random-access seeking inside a track.

The bot can also drive these commands via the AI tool layer — say
"@bot play something chill" or "@bot skip this" and the AI invokes the
same underlying functions. See [AI Chat](ai-chat.md#tool-use).

## Interactive controls

Whenever the bot starts a track (via `!m play`, `!m skip`, or auto-advance),
it sends a "Now Playing" embed with a row of buttons:

- ⏯ Pause / Resume
- ⏭ Skip
- ⏹ Stop and leave
- 🔀 Shuffle the queue
- 🔁 / 🔂 Loop mode (cycles off / track / queue)
- 📋 Show queue

The buttons are gated by two checks:

1. The user pressing them must be **in the same voice channel** as the
   bot.
2. If DJ mode is on for the guild and the user lacks the DJ role
   (and isn't an administrator), the button refuses with an ephemeral
   message.

The "Show queue" button skips the voice-presence check so anyone
listening can peek at what's coming up.

When a track ends and the next one starts automatically, the bot
deletes the previous "Now Playing" message and posts a fresh one for
the new track, so there's only ever one set of controls live in the
channel. The same cleanup runs when a track is skipped — both the
**Skip** button and the `!m skip` text command delete the previous
"Now Playing" message before posting the new one, so manual skips
don't leave orphaned embeds behind.

## Supported sources

Anything `yt-dlp` supports. The most common cases:

- **YouTube videos** — paste a URL, or use a free-text query (the bot
  prefixes the query with `ytsearch1:` so you get the top result).
- **YouTube playlists** — use `!m playlist <url>`. `!m play` on a
  playlist URL only takes the first video, by design (`--no-playlist`
  is set on the single-track path).
- **SoundCloud, Bandcamp, Mixcloud, Vimeo, Twitch VODs, direct media
  URLs** — anything in the `yt-dlp` extractor list.

If `yt-dlp` can extract a single audio stream URL from it, the bot can
play it.

## Audio quality

The bot configures `songbird` for **256 kbps** Opus
(`Bitrate::Bits(256_000)` in `src/music/voice.rs`) and uses the
streaming `YoutubeDl` input. `yt-dlp` is launched with
`-f bestaudio`, so the input is whatever the highest-bitrate audio
stream is at the source — typically Opus directly from YouTube, which
means the bytes flow through to Discord with **no transcoding** at any
point.

Practical consequences:

- CPU footprint is negligible — under a percent on a small VPS — even
  with multiple guilds streaming.
- Quality is bounded by the source. A 96 kbps SoundCloud track is
  still 96 kbps when it reaches your ears.
- There is no normalization, no equalizer, no audio filters. If you
  want loudness normalization you need to add it yourself.

## YouTube cookies

YouTube increasingly demands a logged-in session for anonymous IPs,
particularly:

- Age-restricted videos
- Region-locked videos
- "Sign in to confirm you're not a bot" anti-scraping prompts on
  data-center IPs

The fix is to provide a cookies file. The bot looks for **`cookies.txt`**
in the working directory at startup. The file is gitignored and
intentionally lives per-instance — each bot instance has its own.

### Format

`cookies.txt` is the **Netscape / Mozilla** cookies format. Easiest way
to generate one:

1. Install a browser extension such as
   "Get cookies.txt LOCALLY" (Firefox or Chrome).
2. Log into YouTube in the browser session you control.
3. Use the extension to export cookies for `youtube.com`.
4. Save the file as `cookies.txt` in the instance config directory next
   to `config.toml`.

Use a throwaway YouTube account for this. The bot is going to make API
calls with whatever account you log in as.

### Cookie fallback behaviour

If the cookies file is missing, expired, or otherwise rejected by
YouTube, the bot does not give up. The flow in
`src/music/track.rs::resolve_tracks` is:

1. Run `yt-dlp` with `--cookies cookies.txt`.
2. If the call succeeds, return the result.
3. If it fails *and* the stderr contains a known cookie-error marker
   ("page needs to be reloaded", "sign in to confirm",
   "this helps protect our community", "login required"), retry the
   same query with no `--cookies` flag at all.
4. If the second attempt succeeds, return the result and tell the
   caller to flag the cookies as stale.
5. If the second attempt also fails, surface the error to the user.

When the second attempt is what worked, the bot adds a one-line warning
to chat:

> ⚠ YouTube cookies are expired. Music still works but age-restricted
> content won't. Someone needs to refresh `cookies.txt`.

So you'll know to refresh them, but the bot stays usable in the
meantime.

## Auto-leave

When playback finishes and there is nothing left in the queue, the bot
starts a 5-minute idle timer
(`src/music/voice.rs::start_idle_timer`). If nothing else is queued
within those 5 minutes, the bot leaves the voice channel and clears
its per-guild player state. Any new track started before the timer
fires cancels it.

There is also a separate auto-leave path triggered by **voice state
updates**: if everyone else leaves the voice channel and the bot is
the only remaining occupant, it leaves immediately. See
`src/events/voice_state.rs`.

## Permissions required

The bot needs the standard voice trio in any channel it should be
allowed to play in:

- **Connect** — to join the voice channel
- **Speak** — to transmit audio
- **Use Voice Activity** — so it doesn't have to push-to-talk

If the role you assigned to the bot is missing any of these, joining
will succeed but no audio will be heard, and the bot will not produce
a clean error — it'll just sit silently in the channel. Check role
permissions on a per-channel basis if a specific room misbehaves.

## DJ mode

If the guild has DJ mode enabled (set via `!m djmode` and `!m djrole`,
stored in the database), only members with the DJ role (or
administrators) can use music commands and music buttons. Other
members get a polite refusal.

DJ mode is a **per-guild** setting, not a config-file setting — each
server's admins manage their own.

## Common issues

- **"Sign in to confirm you're not a bot" or "Couldn't find that
  song"** — YouTube needs cookies. Provide a `cookies.txt` (see above).
  Until you do, only non-restricted videos will play.
- **"Video unavailable" or geo-blocked content** — there's nothing the
  bot can do; the source is refusing the request from the bot's egress
  IP. A different region's `cookies.txt` plus a tunneled connection
  might work, but that's outside the bot's scope.
- **Bot joins the channel but no audio plays** — `ffmpeg` is missing
  on the host. The Docker image bundles it; if you're running outside
  Docker, install it via your package manager and make sure it's on
  `PATH`.
- **Audio is choppy or stutters** — rare with passthrough, since CPU
  is barely involved, but possible if the host has heavy disk/network
  contention. Check `htop` and the bot logs for backpressure.
- **The bot's "Now Playing" embed disappears every track change** —
  that's intentional. The bot deletes the previous embed when a new
  track starts so there's only one set of controls live at a time.
- **A long playlist only adds a fraction of its tracks** — the queue
  is capped at 100. Anything past `MAX_QUEUE_LENGTH` is dropped on
  enqueue and the bot tells you how many were added.

## Rate limiting

Every music prefix command and every `music_*` button interaction is
throttled per user through the shared `RateLimiters` infrastructure
at **15 requests / 30 seconds**. That covers all 11 prefix commands
(`play`, `playlist`, `skip`, `stop`, `pause`, `resume`, `queue`,
`nowplaying`, `remove`, `loop`, `shuffle`) and every button on the
"Now Playing" embed (pause/resume, skip, stop, shuffle, loop, show
queue). Hitting the cap returns a "Slow down" reply instead of
executing the action.

The rate limit is in addition to the existing practical limits:

- Discord's voice-gateway rate limits.
- `yt-dlp` startup time (it forks a subprocess per resolve).
- The 100-track queue cap.

If you want stricter throttling than per-user, gate the commands
with DJ mode.

## Cross-references

- [Architecture: Music Pipeline](../architecture/music-pipeline.md) —
  diagrams of the resolve / play / event-handler flow.
- [AI Chat](ai-chat.md) — how to drive music commands through the AI
  tool layer.
- [Instance Config](../configuration/instance-config.md) —
  `command_prefix` and other per-instance settings.
- [Codebase Tour](../development/codebase-tour.md) — where the music
  modules live in the source tree.
