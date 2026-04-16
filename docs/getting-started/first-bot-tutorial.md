# First Bot Tutorial

This page is the slow path. It walks through every step of getting a Discord
bot running, with no assumptions about prior experience. If you have set up a
Discord bot before, [Quickstart](quickstart.md) is the short version of this
same flow.

## Who this tutorial is for

You have heard of Discord bots. You have not set one up yourself, you have
not built a Rust project, and "Docker" is at most a vague impression. That
is fine. By the end of this page you will have a real bot in a real Discord
server, running on hardware you control. You will copy a few files, fill in
a few blanks, and run a few commands.

## What you will build

A Discord bot you fully own:

- Online in a server you control.
- Responds to `!m help` with a command list.
- Plays music in a voice channel with `!m play <url>`.
- Chats with you when you @mention it (if you provide an AI API key).
- Can play Wordle and other small games.
- Stores its data in a PostgreSQL database that runs alongside it.

The whole thing runs in Docker on one machine. There is no website to set
up, no domain to buy, no public IP needed.

## Section 1: Create the Discord application

Discord calls the parent object that owns a bot an "application." Creating
one takes a few clicks in their developer portal.

<!-- screenshot: developer portal landing page -->

1. Open <https://discord.com/developers/applications> and log in with your
   normal Discord account.
2. Click **New Application** in the top right.
3. Give it a name (this becomes the bot's display name; you can change it
   later), accept the terms, and click **Create**.

You are now on the **General Information** page.

<!-- screenshot: general information page with Application ID highlighted -->

Find the **Application ID** and copy it. Save it in a notes file labeled
`CLIENT_ID`. You will need it later.

### Get the bot token

The token is the secret password your code uses to log in as this bot.

1. Click **Bot** in the left sidebar.
2. In the **Token** section, click **Copy** (or **Reset Token** if no token
   is shown), and save the value as `DISCORD_TOKEN`.

Treat this string like a banking password. Do not paste it in chat, do not
screenshot it, do not commit it to git. The configuration files you will
create later are gitignored specifically so a slip of the keyboard cannot
leak it. If you ever do leak it, click **Reset Token** again and the old
token stops working immediately.

### Turn on the privileged intents

Still on the **Bot** page, scroll to **Privileged Gateway Intents**.

<!-- screenshot: privileged intents toggles -->

Turn on **Server Members Intent** and **Message Content Intent**. Leave
**Presence Intent** off. The bot needs Server Members so it can react to
joins (welcome and auto-role features), and Message Content so it can read
@mentions for AI chat and parse text commands like `!m help`.

Click **Save Changes** at the bottom.

## Section 2: Invite the bot to your server

The application exists, but it is not in any server yet.

1. Click **Installation** in the sidebar (or **OAuth2 → URL Generator** on
   older portals).
2. Under **Scopes**, check `bot`. (`applications.commands` isn't needed —
   the bot has no slash commands — but checking it is harmless if your
   workflow already includes it.)
3. Under **Bot Permissions**, check at minimum: View Channels, Send
   Messages, Embed Links, Attach Files, Read Message History, Connect,
   Speak, Manage Roles, Kick Members, Ban Members, Moderate Members. You
   can drop any of these later if you do not use the matching feature.
4. Copy the URL at the bottom, paste it into a new tab, choose your server
   (you must have **Manage Server** there), and click **Authorize**.

The bot now appears in your server's member list with a grey dot. The dot
turns green once you start the code.

### Get the server ID

1. In Discord, open **User Settings → Advanced** and turn on **Developer
   Mode**.
2. Right-click your server icon and choose **Copy Server ID**.
3. Save it as `GUILD_ID`.

You should now have three values:

```
DISCORD_TOKEN=...
CLIENT_ID=...
GUILD_ID=...
```

## Section 3: Install Docker

Docker runs the bot. The bot ships as a container so you do not have to
install Rust, set up a database server, or worry about library versions.

**Linux.** Use the convenience script at <https://get.docker.com>, or your
distribution's packages: `apt install docker.io docker-compose-v2` on
Debian/Ubuntu, `dnf install docker docker-compose` on Fedora. Add yourself
to the `docker` group with `sudo usermod -aG docker $USER`, then log out
and back in.

**macOS.** Install [Docker
Desktop](https://www.docker.com/products/docker-desktop/) and launch it
once after install.

**Windows.** Install Docker Desktop with the WSL2 backend, install a WSL2
Linux distribution from the Microsoft Store (Ubuntu is fine), and run all
the commands in this tutorial from inside that distribution. Native
PowerShell is technically possible but not worth the friction.

Verify in a terminal:

```bash
docker --version
docker compose version
```

Both should print a version.

## Section 4: Clone and configure

Fetch the code:

```bash
git clone https://github.com/MrMcEpic/discord-bot-rs.git
cd discord-bot-rs
```

Make a copy of the example instance directory:

```bash
cp -r instances/example instances/mybot
cp instances/mybot/.env.example instances/mybot/.env
```

`mybot` is just a directory name. Pick whatever you like.

Open `instances/mybot/.env` in any text editor and paste in the three
values you saved earlier:

```bash
DISCORD_TOKEN=...
CLIENT_ID=...
GUILD_ID=...
```

A few lines down, change `DB_SCHEMA` to match your instance name:

```bash
DB_SCHEMA=mybot
```

That is the minimum. Save the file.

Open `instances/mybot/config.toml`. The first few lines control identity:

```toml
bot_name = "My Bot"
command_prefix = "!"
```

Change `bot_name` to whatever you want shown in help text. Leave
`command_prefix` as `!` for now; the rest of the documentation assumes it.
Save and close.

If you want AI chat, also paste a `DEEPSEEK_API_KEY` or `GEMINI_API_KEY`
into `.env`. Both are free to start with — see
[Prerequisites](prerequisites.md) for signup links. You can add them later.

One last required step: generate an MCP auth token. The bundled stack
won't start without one, because the embedded MCP server and the
`mcp-gateway` sidecar both refuse to run without a shared secret set.
Generate one random value and install it in two places: `MCP_AUTH_TOKEN`
in `instances/mybot/.env` (the bot reads it) and `MCP_GATEWAY_AUTH_TOKEN`
in a new `.env` at the repo root (Docker Compose reads it and feeds it to
the gateway). Both must hold **the same** value — the gateway uses it as
the bearer when reaching the bot, so a mismatch locks the two out of each
other:

```bash
TOKEN=$(openssl rand -hex 32) && \
  sed -i.bak "s|^MCP_AUTH_TOKEN=.*|MCP_AUTH_TOKEN=$TOKEN|" instances/mybot/.env && \
  rm instances/mybot/.env.bak && \
  echo "MCP_GATEWAY_AUTH_TOKEN=$TOKEN" >> .env
```

If you prefer editing by hand, run `openssl rand -hex 32`, copy the
output, paste it after `MCP_AUTH_TOKEN=` in `instances/mybot/.env`, and
add a single line `MCP_GATEWAY_AUTH_TOKEN=<paste>` to a new file called
`.env` at the repo root (next to `docker-compose.yml`).

## Section 5: First run

Start the stack:

```bash
INSTANCE_DIR=./instances/mybot docker compose up -d
```

The first run takes a few minutes because Docker has to download PostgreSQL
and build the bot from source. Once it finishes, you get your shell prompt
back.

Tail the logs:

```bash
docker compose logs -f bot
```

You will see, roughly in order:

1. Postgres starting up and reporting it is ready to accept connections.
2. The bot connecting to Postgres and creating its schema
   ("Database initialized (schema: mybot)").
3. The bot loading its instance config
   ("Instance config loaded: My Bot (prefix: !)").
4. The bot connecting to Discord ("Starting bot..." → shard running →
   "My Bot is connected! (ID: ...)").
5. The MCP server binding to port 9090. From here on the bot is online.

Press `Ctrl+C` to stop tailing. The bot keeps running. To stop everything,
run `docker compose down` from the repo root.

## Section 6: Test it

Open Discord. Your bot should now have a green dot in the member list.

In any channel the bot can see, type:

```
!m help
```

You should get a help embed listing the available commands. discord-bot-rs
uses prefix commands only — there are no slash commands — so `!m help` is
how you discover everything.

## Section 7: What to try next

Now that the bot works, here are some things to play with.

- **Customize the personality.** Edit `instances/mybot/personality.txt` and
  put whatever you want the bot's voice to be. Restart with
  `docker compose restart bot`.
- **Try AI chat.** With an API key set, @mention the bot and ask it
  something. It will reply in the personality you defined.
- **Try music.** Join a voice channel, then run `!m play <YouTube URL>` in
  any text channel. `!m skip` skips, `!m queue` shows the queue.
- **Try Wordle.** Run `!m wordle` in a channel.
- **Read [Features](../features/index.md)** to see everything the bot can
  do and how to enable the optional pieces.

## Section 8: Common first-time mistakes

If something is not working, the issue is almost always one of these.

**Bot stays grey (offline) and never goes green.** The token is wrong, the
privileged intents are off, or both. Check `docker compose logs bot` —
"Invalid Token" means reset the token and paste the new value; "Disallowed
intents" means go back to the Bot page in the portal and verify both Server
Members Intent and Message Content Intent are enabled.

**`!m help` returns nothing.** The bot needs **Message Content Intent**
enabled in the Discord developer portal to read prefix commands. If you
skipped that in Section 1, re-enable it and restart the bot with
`docker compose restart bot`. Also check that the bot has permission to
read and send messages in the channel you are typing in.

**Bot replies to `!m help` but not to @mentions.** AI chat is disabled
because no API key is set. Add `DEEPSEEK_API_KEY` or `GEMINI_API_KEY` to
`.env`, then `docker compose restart bot`.

**Postgres keeps restarting.** Run `docker compose logs postgres`. Usually
this is a disk space issue or a corrupted volume from an unclean shutdown.
`docker compose down` followed by `docker compose up -d` resolves most
transient issues.

**The first build hangs or fails.** The Rust build downloads many crates
and compiles for several minutes. If your machine has very little RAM
(under 2 GB), the build may run out of memory. The smallest VPS that
reliably builds the project is 2 GB RAM with at least 2 GB of swap.

If none of these apply, run through
[Verifying Your Setup](verifying-your-setup.md) for a more thorough
diagnostic, or open an issue on GitHub.
