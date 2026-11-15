# Web Search

The AI's `web_search` tool (used for current events, prices, "look this up",
and the reasoner's pre-flight research) goes through a list of search
**backends**, tried in order. The first backend that returns results wins; one
that errors or comes back empty falls through to the next.

| Backend      | Needs                              | Notes |
|--------------|------------------------------------|-------|
| `searxng`    | a SearXNG instance (`SEARXNG_URL`) | **Recommended.** Self-hosted metasearch over its JSON API. It aggregates several engines (Brave, Google CSE, ...), so one engine blocking your server's IP doesn't take search down. The bundled `docker-compose.yml` runs one. |
| `duckduckgo` | nothing                            | Scrapes DuckDuckGo's keyless HTML endpoint. Zero setup, but DuckDuckGo now answers most automated traffic with a bot challenge, so on its own it often returns nothing. |

Default order: `searxng`, then `duckduckgo`. `searxng` is skipped when no URL
is configured, so a bot without SearXNG behaves like it did before 0.19.

## Docker Compose (bundled SearXNG)

`docker-compose.yml` includes a `searxng` service and sets
`SEARXNG_URL=http://searxng:8080` on the bot. The service has **no published
port**: only containers on the compose network can reach it. Its settings live
in [`searxng/settings.yml`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/searxng/settings.yml)
(mounted read-only):

- `search.formats` includes `json`. Without it SearXNG answers the bot's
  queries with HTTP 403.
- The bot-detection `limiter` is off, since the only client is the bot.
- SearXNG's own DuckDuckGo engine is disabled, because it hits the same
  CAPTCHA and only adds latency.

`SEARXNG_SECRET` is optional. If unset, the container generates a random
secret on each start, which is fine because the bot keeps no SearXNG sessions.

For multi-instance deployments, one `searxng` service is enough: point every
bot service at it with the same `SEARXNG_URL`.

## Running without Docker

Run SearXNG any way you like (see the
[SearXNG install docs](https://docs.searxng.org/admin/installation.html)),
enable the JSON format, and set `SEARXNG_URL` in the instance's `.env`:

```bash
SEARXNG_URL=http://127.0.0.1:8888
```

Don't point the bot at a public SearXNG instance. Most disable the JSON API
and rate-limit automated clients.

## `[search]` section (instance config)

All keys are optional:

```toml
[search]
backends = ["searxng", "duckduckgo"]   # order to try; drop one to disable it
searxng_url = "http://searxng:8080"    # overrides the SEARXNG_URL env var
timeout_secs = 10                      # per-backend request timeout
```

An unknown backend name is a startup error. If the list ends up with no usable
backend (e.g. only `searxng` but no URL), startup logs a warning and the tool
reports failures.

## Diagnostics

At startup the bot logs the active backends:

```text
Web search backends: [Searxng, Duckduckgo] (searxng at http://searxng:8080)
```

When search fails, the model now receives the real reason (e.g.
`Search failed: searxng: request failed: ...; duckduckgo: DuckDuckGo served a
bot-detection challenge instead of results (blocked)`) instead of a bare "No
results found." It can then tell the user that search is broken rather than
claiming nothing exists. Per-backend failures are also logged at `WARN`.

To check SearXNG directly from inside the compose network:

```bash
docker compose exec bot curl -s "http://searxng:8080/search?q=test&format=json" | head -c 300
```
