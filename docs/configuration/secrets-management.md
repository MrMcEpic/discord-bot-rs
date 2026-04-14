# Secrets Management

A Discord bot token grants full control over your bot user — anyone who has it can read messages the bot can read, post anywhere it can post, and join voice channels on its behalf. Provider API keys cost real money if abused. Database credentials open the door to your whole instance's data. Leaked secrets are by far the most common way bot projects get pwned, and they almost always leak the same way: someone commits an `.env` file or pastes a token into a public log.

This page describes how this project keeps secrets out of git by default, how to verify that's actually the case for your fork, and what to do if something escapes anyway.

## The default posture

The repo's `.gitignore` excludes secrets at the file level, not the line level. The relevant entries are:

```
.env
instances/*/.env
!instances/*/.env.example
cookies.txt
instances/*/cookies.txt
```

That covers three things:

1. The root `.env` (used in some legacy local-dev flows).
2. Any per-instance `.env` under `instances/<name>/`.
3. The negation `!instances/*/.env.example` keeps the documented templates checked in, so the docs and onboarding still work.
4. Any `cookies.txt` (used by the music feature for YouTube authentication) at the repo root or per-instance.

If you cloned the repo cleanly and only created `.env` files at the documented paths, none of them will ever be staged by `git add .`.

## Verifying your `.gitignore`

Before your first push, confirm that git is actually ignoring your `.env`:

```bash
git check-ignore -v instances/yourbot/.env
```

You should see a line pointing at `.gitignore` and the matching pattern. If you get nothing, the file is *not* ignored — investigate before pushing. The likeliest cause is that you put your env file somewhere unexpected (e.g. `instances/yourbot/secrets.env`), in which case either move it to `.env` or extend `.gitignore` to cover the new path.

## Checking git history

If you're worried something already snuck in, search the whole history:

```bash
git log -p --all | grep -iE "DISCORD_TOKEN|API_KEY|SECRET|password" | head -50
```

This is noisy by design — it'll match docs and example files too. Read each hit carefully. If you find an actual token, a real API key, or anything else that grants access, treat it as leaked and follow the [recovery flow](#what-to-do-if-a-secret-leaks) below. Rotation is your only real fix; deleting the file in a new commit does not remove the value from history.

## Local development

For local dev, copy the example file:

```bash
cp instances/example/.env.example instances/example/.env
```

Then fill in the values. Never copy your real `.env` into chat, into a paste site, or into a screenshot. If you need to share configuration with a teammate, send them the field list and have them generate their own values, or use a secret-sharing tool like [age](https://github.com/FiloSottile/age) or your password manager's sharing feature.

If you use a code editor with cloud sync, double-check that it isn't quietly mirroring `.env` files to a remote — some editors (and some "AI assistant" extensions) read everything in the workspace by default.

## Docker Compose: `env_file` vs. `environment:`

`docker-compose.yml` uses `env_file: instances/yourbot/.env` rather than putting values in an `environment:` block. This matters because `docker-compose.yml` is checked into the repo, and `environment:` values live in plain sight inside it. `env_file` keeps the path to the secrets in the compose file but the actual values stay in the gitignored `.env`.

The same logic applies if you ever feel tempted to bake secrets into a `Dockerfile` with `ENV` or `ARG`. Don't. They end up in image layers, which are visible to anyone with pull access to the image.

## Docker secrets in production

For production deployments, Docker has a [secrets](https://docs.docker.com/engine/swarm/secrets/) primitive that mounts secret values as files inside the container, owned by root and readable only by the process. It's a step up from `env_file` because the secret never lives on disk in plaintext outside the swarm raft store, and because rotating a secret rotates it everywhere it's mounted.

For a single-host Compose deployment, the marginal benefit over a properly permissioned `.env` (see [Production hardening](#production-hardening)) is small. For a Swarm or Kubernetes deployment, secrets are the right answer. The bot itself reads from environment variables, so adapting it just means setting up a small entrypoint that exports the contents of mounted secret files into the environment before launching the binary.

## Rotating secrets

- **Discord token.** Open the Developer Portal → your application → *Bot* → *Reset Token*. The new token immediately invalidates the old one. Update `.env`, restart the bot, done.
- **DeepSeek / Gemini / Finnhub API keys.** Each provider has a key-management page. Generate a new key, update `.env`, restart, then revoke the old key in the dashboard.
- **Database password.** Update Postgres (`ALTER USER discord_bot WITH PASSWORD '<new>';`), update `DATABASE_URL` in `.env`, restart the bot.
- **`MCP_AUTH_TOKEN` / `MCP_GATEWAY_AUTH_TOKEN`.** Generate a new value (`openssl rand -hex 32`), update `.env`, restart the bot and the gateway.

There is no live reload for any of these — restarting the container is the rotation step.

## What to do if a secret leaks

If you discover an exposed secret, work in this order:

1. **Generate a replacement before revoking the leaked one.** Your running bot still has a valid token; you don't want to take it offline before the replacement is ready.
2. **Update `.env`** with the new value.
3. **Restart the bot.** Confirm it comes back up cleanly with the new credentials.
4. **Revoke the old secret** in the provider dashboard. For Discord, *Reset Token* invalidates the previous one automatically.
5. **Audit logs** for the window the secret was live. Look for unexpected message activity, role changes, channel creations, or API calls.
6. **If the secret was committed to git**, the value is in history and is effectively public. Use [`git filter-repo`](https://github.com/newren/git-filter-repo) to purge it, force-push the rewritten history, and ask collaborators to re-clone. Note that anyone who already cloned (including caches like the GitHub web UI) still has it — rotation is what actually fixes the leak. History rewriting is just hygiene.

## Production hardening

A few quick wins beyond gitignoring `.env`:

- **Restrict database access** to the bot user. Don't reuse a Postgres superuser. If you're running multi-instance, use the same dedicated `discord_bot` user but rely on the per-schema `search_path` for isolation.
- **Set tight permissions on `.env`.** `chmod 600 instances/yourbot/.env` so only the owner can read it. The Compose service runs as a non-root user; make sure that user is the owner.
- **Don't log environment variables at startup.** The bot doesn't currently log secrets — it logs only the bot name, command prefix, and which feature modules are enabled — but if you add logging, never dump the full env or the full `Config` struct.
- **Use `MCP_AUTH_TOKEN` whenever you bind MCP outside `127.0.0.1`.** An empty token on a publicly bound port hands programmatic control to anyone who finds the address. See [MCP Exposure](../deployment/mcp-exposure.md).
- **Run the container as a non-root user.** The shipped image already does this.
- **Keep dependencies up to date.** `cargo update` and rebuild on a regular cadence; security advisories for transitive crates show up via `cargo audit` if you wire it into CI.

For a more complete production walkthrough, see the [Production Checklist](../deployment/production-checklist.md).
