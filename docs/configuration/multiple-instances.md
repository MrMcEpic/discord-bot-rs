# Multiple Instances

discord-bot-rs is designed so a single binary, a single `docker-compose.yml`, and a single Postgres database can host as many independent bots as your hardware can spare. "Multiple instances" here means exactly that: several Discord bot identities, each with its own config and database schema, running side by side as separate processes.

## When to use it

- You run more than one Discord community and want one machine to host every bot.
- You want a dev or staging instance running alongside production so you can test changes without risking the live bot.
- You're hosting bots for friends and want to consolidate maintenance.

If you only run one bot, you don't need any of this — the example instance is already a complete single-instance deployment. Come back here when you actually have a second bot to add.

## The recipe

The flow is "copy the example, edit two files, edit Compose, restart." Concretely:

1. **Copy the example directory.**

   ```bash
   cp -r instances/example instances/bot2
   cp instances/bot2/.env.example instances/bot2/.env
   ```

2. **Edit `instances/bot2/.env`.** At minimum, change:

   - `DISCORD_TOKEN` — the new bot's token from the Discord Developer Portal
   - `CLIENT_ID` — the new application's client ID
   - `GUILD_ID` — the snowflake of the new bot's home server
   - `DB_SCHEMA=bot2` — must be unique across instances

   Leave `DATABASE_URL` pointing at the same Postgres service as the first bot. If you set any optional API keys (DeepSeek, Gemini, Finnhub, Minecraft), each instance gets its own — they don't share keys unless you make them.

3. **Edit `instances/bot2/config.toml`.** Set `bot_name` to something distinct (it shows up in logs and helps you tell which instance is which when tailing) and adjust `command_prefix`, feature flags, and feature sub-sections to match what you want this bot to do.

4. **Edit `instances/bot2/personality.txt`.** Even if the persona is the same as your first bot, edit the file so the loader has something non-empty to read. Most people want a different persona per community anyway.

5. **Duplicate the bot service in `docker-compose.yml`.** Copy the existing `bot` service block and rename the copy to `bot2`. Update its `env_file` to point at `instances/bot2/.env`, update its volume mount so `instances/bot2` becomes `/config` inside the new container, and change the container name. The `image`, `depends_on: postgres`, and `restart` settings can stay identical — both containers run the same image with different config mounted.

6. **Update the `mcp-gateway` service.** Add the new instance to the gateway's `INSTANCES` env var so it knows where to route requests for `bot2`. The gateway reads this list at startup; restarting the gateway picks up new entries.

7. **Bring it up.**

   ```bash
   docker compose up -d bot2
   docker compose restart mcp-gateway
   ```

You should see `bot2`'s startup logs report its bot name, prefix, and the feature modules it enabled. From Discord's perspective the second bot is wholly independent of the first.

## Shared resources

A multi-instance deployment shares three things across all bots:

- **PostgreSQL.** One Postgres container, one database, but each instance writes to its own schema. The schema is set by `DB_SCHEMA` in the instance's `.env` and applied via `SET search_path` on every new connection (see `src/db/mod.rs`). Tables, sequences, and migrations all live inside the schema, so two instances with `DB_SCHEMA=bot1` and `DB_SCHEMA=bot2` cannot see each other's data.
- **MCP gateway.** A single gateway service routes MCP requests to the right instance based on the URL path. Each instance still runs its own embedded MCP server inside its container; the gateway just provides a single externally addressable endpoint. See [MCP Gateway Routing](../architecture/mcp-gateway-routing.md).
- **Docker network.** All bots and the gateway sit on the default Compose bridge network. They can reach Postgres and each other by service name, but Discord-side they're completely independent — each one owns its own gateway connection and Discord application.

Everything else — config, personality, runtime memory, async tasks — is per-instance.

## Concurrency and resource limits

Each instance is a separate process running its own Tokio runtime, so CPU and RAM scale roughly linearly with the number of instances. There is no shared in-process state between bots, which is the point — but it also means there's no economy of scale on memory: two bots use about twice as much RAM as one bot.

Practical sizing notes:

- A modern Pi 4 (4GB) comfortably runs two or three bots plus the bundled Postgres and gateway.
- The biggest variable is music: voice connections and ffmpeg pipelines dominate memory and CPU when active. A bot that never plays music is much cheaper than one with three concurrent voice channels.
- Postgres is the smallest part of the budget unless you have many tens of thousands of tracked messages or game states.

When you start hitting limits, the next step up is usually splitting Postgres onto a dedicated host (or a managed service) rather than splitting the bots themselves.

## Gotchas

- **`DB_SCHEMA` must be unique per instance.** Two instances pointed at the same schema will corrupt each other's state. There is no defensive check for this — you have to get it right.
- **The MCP server runs in-process** and is reached over the Docker network from the gateway. You don't need to expose its port to the host unless you also want direct access from outside Compose. If you do expose it, every instance needs a different host port (e.g. `9091:9090`, `9092:9090`).
- **Docker container names must be unique.** If you copied the `bot` service block without renaming the `container_name`, Compose will complain. Pick a name per instance.
- **Health checks per instance are fine.** Each container's healthcheck talks to its own MCP server on `127.0.0.1:9090` inside the container, and that doesn't conflict with anything because each container has its own loopback.
- **Discord rate limits are per-token, not per-host.** Running multiple bots on one host doesn't multiply the rate limit budget for any single bot, but bots don't share rate limits with each other.

## See also

- [Multi-Instance Model](../architecture/multi-instance-model.md) — the architectural picture, with a diagram of how the pieces fit.
- [Multi-Instance Deployment](../deployment/multi-instance-deployment.md) — the full Compose layout and operational runbook.
- [MCP Gateway Routing](../architecture/mcp-gateway-routing.md) — how the gateway maps requests to instances.
