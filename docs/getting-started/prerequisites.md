# Prerequisites

Before you can run the bot, you need three things: a Discord application
(this gives you the bot token and identity), Docker with the Compose plugin
(this runs the bot), and a host to run them on. None of these take very long,
but the Discord developer portal has some non-obvious clicks, so this page
walks through every one.

If you would rather follow a step-by-step version with more hand-holding,
see [First Bot Tutorial](first-bot-tutorial.md). If you have done all this
before, skim and skip to [Quickstart](quickstart.md).

## Discord application

A Discord "application" is the parent object for a bot. It owns the token,
the client ID, the OAuth scopes, and the assets (avatar, name, description).
You create one through Discord's developer portal.

### Create the application

1. Open <https://discord.com/developers/applications> and sign in.
2. Click **New Application** in the top right.
3. Give it a name. This is the public name your bot will appear under in
   Discord, so pick something you are comfortable with. You can change it
   later.
4. Accept the developer terms and click **Create**.

You are now on the application's **General Information** page. Two things on
this page matter:

- The **Application ID** under the application name. Copy this and save it
  somewhere safe. This is your `CLIENT_ID`.
- (Optional) The icon and description, which become your bot's profile in
  Discord. Set them now or later, it does not affect anything technical.

### Get the bot token

1. In the left sidebar, click **Bot**.
2. Scroll to the **Token** section. If a token is already displayed, copy it.
   If not, click **Reset Token** and copy the new value.
3. Save it somewhere safe. This is your `DISCORD_TOKEN`.

The token is a password. Anyone who has it can control your bot. Do not
commit it to git, do not paste it into chat, and do not share screenshots of
it. The bot's `.env` file (which you will create later) is gitignored
specifically to keep this safe.

If you ever leak it, come back to this page and click **Reset Token** again.
The old one stops working immediately.

### Enable privileged intents

Still on the **Bot** page, scroll to **Privileged Gateway Intents**. Two
intents must be on:

- **Server Members Intent** — required for member join events, which the
  auto-role and welcome features depend on.
- **Message Content Intent** — required for the AI chat feature, which reads
  the text of messages that @mention the bot, and for any prefix command
  (`!m help`, `!m play`, etc.) that needs to read what the user typed.

You can leave **Presence Intent** off; nothing in this bot uses it.

Click **Save Changes** at the bottom of the page.

## Invite the bot to your server

The bot exists, but it is not in any server yet. You need to generate an
OAuth invite URL.

1. In the left sidebar, click **Installation** (newer portals) or
   **OAuth2 → URL Generator** (older portals).
2. Under **Scopes**, check `bot` only. (The bot has no slash commands, so
   `applications.commands` isn't required — checking it is harmless.)
3. Under **Bot Permissions**, check the permissions the features you plan to
   use require:
   - **View Channels**, **Send Messages**, **Embed Links**, **Attach Files**,
     **Read Message History** are needed for almost everything. Always include
     these.
   - **Connect** and **Speak** are needed for music.
   - **Manage Roles** is needed for auto-role, join role, donator sync, and
     chargeback alerts. The bot's own role must also be ranked above any role
     it tries to assign.
   - **Kick Members**, **Ban Members**, and **Moderate Members** are needed
     for the moderation commands.
4. Copy the generated URL at the bottom of the page.
5. Paste it into your browser, pick the server you want the bot in, and
   click **Authorize**.

You can drop any of those permissions if you do not plan to use the
corresponding feature. The bot will simply log a warning the first time a
disabled feature tries and fails to do its job.

## Get the GUILD_ID

The bot needs to know which Discord server is its primary one. That is the
"guild ID."

1. In Discord, open **User Settings → Advanced** and turn on **Developer
   Mode**.
2. In your server list, right-click the server you just invited the bot to
   and choose **Copy Server ID**.
3. Save it. This is your `GUILD_ID`.

## Docker and Docker Compose

The bot ships as a set of Docker images. You need Docker Engine and the
Compose plugin (Compose v2, invoked as `docker compose`, not the legacy
`docker-compose` script).

- **Linux:** install from your distribution's package manager, or run the
  convenience script at <https://get.docker.com>. Most distributions package
  the Compose plugin separately as `docker-compose-plugin` or
  `docker-compose-v2`.
- **macOS or Windows:** install [Docker
  Desktop](https://www.docker.com/products/docker-desktop/). Compose ships
  with it. On Windows, run everything from inside a WSL2 distribution; native
  PowerShell has not been tested.

Verify the install:

```bash
docker --version
docker compose version
```

Both commands should print a version. If `docker compose version` fails but
`docker-compose --version` works, you have the legacy Python plugin. Install
the v2 plugin and use `docker compose` (with a space) from then on.

## A host to run on

Anything that runs Docker will run the bot. Concrete options:

- A Raspberry Pi 4 (4 GB or 8 GB) on your home network. Plenty for a single
  instance.
- A small VPS (1 vCPU, 2 GB RAM) from any provider. Hetzner, Vultr, and
  DigitalOcean all have suitable boxes for around five dollars a month.
- Your laptop, for testing. The bot does not care if it runs intermittently
  while you are developing.

Plan for about 5 GB of free disk for Docker images, the PostgreSQL volume,
and any cached audio.

## Optional external services

These are only required if you want the corresponding feature. All can be
added later by editing your `.env` and restarting.

- **DeepSeek API key** — primary AI chat provider. Free credits on signup.
  Sign up at <https://platform.deepseek.com/>.
- **Gemini API key** — fallback AI chat provider, used automatically when
  DeepSeek errors. Free tier is generous. Get a key at
  <https://aistudio.google.com/apikey>.
- **Finnhub API key** — required for the virtual stock trading game. Free
  tier is enough. Sign up at <https://finnhub.io/>.
- **Minecraft companion plugin** — required only if you plan to use the
  Minecraft verification or donator sync features. See
  [Minecraft: Verify](../features/minecraft-verify.md).

If you are not sure whether you want a feature, leave its key blank and
enable it later.

## What you do NOT need

A few things people commonly ask about that you can ignore:

- **An external PostgreSQL** — the bundled `docker-compose.yml` includes one.
- **A domain name or DNS records** — the bot connects out to Discord; nothing
  needs to reach it from the public internet.
- **A reverse proxy or TLS certificate** — only relevant if you intend to
  expose the embedded MCP server to a network outside `localhost`. See
  [MCP Exposure](../deployment/mcp-exposure.md).
- **A specific Linux distribution** — anything that runs current Docker
  works.

## Next step

Once you have your token, client ID, guild ID, and Docker installed, head to
[Quickstart](quickstart.md).
