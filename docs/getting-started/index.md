# Getting Started

This section takes you from "I have a Discord server" to "I have my own bot
running in it." It is split into five pages:

- [Prerequisites](prerequisites.md) lists what you need to gather before
  touching the code: a Discord application, a token, Docker, and a host to run
  on.
- [Quickstart](quickstart.md) is the fast path. If you have done Discord bot
  setup before, this is the only page you need.
- [First Bot Tutorial](first-bot-tutorial.md) is the slow path. It walks
  through every step with no assumed knowledge, including screenshots of the
  Discord developer portal.
- [Verifying Your Setup](verifying-your-setup.md) is the post-deploy
  checklist. Run it once after your first start to make sure each piece is
  healthy.

## When to use discord-bot-rs

This project is a good fit if any of the following describe you:

- You want to self-host a Discord bot rather than rent a hosted one, and you
  are comfortable running a Docker container on a Linux box.
- You want one codebase that can run several distinct bots (different names,
  different personalities, different feature sets) from a single deployment.
- You want a bot with batteries included: AI chat, music, games, moderation,
  and a Minecraft integration, all behind feature flags so you can switch off
  what you do not need.
- You want a Rust codebase you can read, fork, and extend without fighting a
  framework.

## When NOT to use it

Reach for something else if:

- You just want a moderation or music bot for one server and you do not want
  to run anything yourself. A hosted bot from top.gg will be faster.
- You want a JavaScript or Python codebase to hack on. Sapphire.js,
  discord.py, and pycord are all excellent and have larger communities.
- You need a single small bot and the overhead of Docker plus PostgreSQL
  feels like too much. A 100-line script is the right answer for that.

## System requirements

The bot is tested on Linux. macOS and Windows (via WSL2) work as long as
Docker Desktop is running. You will need:

- Docker Engine 20.10 or newer with the Compose plugin.
- About 2 GB of free RAM. The bot itself is small; most of the budget goes to
  PostgreSQL and ffmpeg during music transcoding.
- About 5 GB of free disk for the Docker images, the database volume, and
  yt-dlp cache.
- Outbound network access to Discord, your AI provider, and (if you enable
  music) YouTube.

A Raspberry Pi 4 with 4 GB of RAM is enough for a single instance. A $5/month
VPS is enough for several. Your laptop is enough for testing.

## Time investment

Plan on about 10 minutes from `git clone` to a running bot if you have set up
a Discord application before and you already have Docker installed. Budget
closer to 25 minutes if it is your first time, mostly because the Discord
developer portal has a few non-obvious clicks.

## Next step

Head to [Prerequisites](prerequisites.md) to gather what you need before the
first command.
