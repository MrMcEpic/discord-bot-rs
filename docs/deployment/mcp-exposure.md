# MCP Exposure

The bot embeds a Model Context Protocol server, and the Compose
stack ships with a gateway that fronts one or more of those
servers. Both are powerful — they can ban members, delete channels,
post messages, edit roles. This page is about deciding when and how
to make those endpoints reachable from outside the host without
handing administrator rights to the internet.

If you have not read [MCP Server](../features/mcp-server.md) yet,
do that first — it covers what the tools do and how a client
connects. This page is about the network and authentication layer.

## Two deploy shapes

There are two reasonable shapes for an MCP-exposing deployment, and
the rest of this page assumes you have picked one:

### Shape A — loopback-only bot, no gateway

A single bot, no `mcp-gateway` sidecar. The bot's MCP server binds
`127.0.0.1` and is reachable only from inside the bot's container
(or from the host, if you publish the port to `127.0.0.1`). This is
the right default if you are not using the gateway and you only
need MCP locally. `MCP_AUTH_TOKEN` is **optional** in this shape;
loopback binds are allowed to ship without a token.

Set in `.env`:

```
MCP_BIND_ADDR=127.0.0.1
MCP_AUTH_TOKEN=        # optional on loopback
```

### Shape B — bundled Compose with the gateway

The shape the shipped `docker-compose.yml` is built for. The
`mcp-gateway` sidecar publishes `127.0.0.1:9100` on the host and
reaches each bot at `http://<bot-service>:9090` over the internal
Docker bridge network. Because the gateway is in a different
container from the bot, the bot must bind on a non-loopback
interface inside its own container (`MCP_BIND_ADDR=0.0.0.0`),
otherwise the gateway cannot reach it. The shipped
`instances/example/.env.example` already does this.

`MCP_AUTH_TOKEN` is **mandatory** in this shape. The bot refuses
to start if `MCP_BIND_ADDR` is non-loopback and `MCP_AUTH_TOKEN`
is empty — see "Hard auth requirements" below.
`MCP_GATEWAY_AUTH_TOKEN` is mandatory always; the gateway refuses
to start without it.

Set in `.env`:

```
MCP_BIND_ADDR=0.0.0.0
MCP_AUTH_TOKEN=$(openssl rand -hex 32)         # required
MCP_GATEWAY_AUTH_TOKEN=$(openssl rand -hex 32) # required (gateway service)
```

Even in Shape B nothing is exposed to the internet by default —
the gateway publishes only on `127.0.0.1:9100` on the host, so
the MCP endpoint is reachable from a Claude Code on the same host
(`http://localhost:9100/mcp`), from anyone you `ssh -L
9100:localhost:9100` to, and from nothing else. The "expose this
to a remote machine" patterns later on this page apply on top of
Shape B.

## Hard auth requirements

The bot and the gateway both refuse to start in obviously unsafe
configurations:

- **Bot:** if `MCP_AUTH_TOKEN` is empty *and* `MCP_BIND_ADDR` is
  not a loopback address, startup aborts with an error pointing
  at `src/mcp/mod.rs`. This is to prevent the easy mistake of
  switching to `0.0.0.0` (for a Compose deploy, or any
  cross-container reach) and forgetting to set a token.
- **Gateway:** if `MCP_GATEWAY_AUTH_TOKEN` is empty, startup
  aborts unconditionally. The gateway's whole job is to be
  reachable from outside its container, so there is no loopback
  escape hatch.

Bearer comparison on both is constant-time (via the `subtle`
crate), so a brute-force probe cannot distinguish "token wrong by
the first byte" from "token wrong by the last byte" through
timing.

Both servers also cap incoming request bodies at **64 KiB**
(`RequestBodyLimitLayer`), so a misbehaving or malicious client
cannot tie up RAM with an oversized payload, and malformed JSON
returns a proper JSON-RPC parse-error response rather than
crashing the connection.

If that covers your use case, **stop here**. The defaults plus a
token are the right answer for most operators. The rest of this
page is about the cases where it is not.

## When to expose MCP

You only need to think about exposing MCP when:

1. Your MCP client (an AI agent, a CI runner, a remote operator's
   Claude Code) lives on a machine that is not the bot host, and
2. You cannot reasonably tunnel to the bot host first.

Case 1 is common — your laptop is not the bot host. Case 2 is
rare; SSH port forwarding is usually the cheapest answer and you
should reach for it first.

## Pattern 1: SSH tunnel (recommended)

For an operator's laptop reaching the gateway from anywhere:

```bash
ssh -L 9100:localhost:9100 user@bot-host
```

The gateway is now reachable as `http://localhost:9100/mcp` on the
laptop, encrypted over SSH, authenticated by SSH's existing key
infrastructure. No changes to the Compose file, no exposed ports,
no token to manage. As long as you have SSH access to the host,
you have MCP access.

Configure your MCP client (e.g. Claude Code's `~/.claude.json`):

```json
{
  "mcpServers": {
    "discord": {
      "type": "http",
      "url": "http://localhost:9100/mcp"
    }
  }
}
```

This is the right answer 90% of the time. Use it before you
consider anything else.

## Pattern 2: WireGuard / Tailscale

For a small group of operators or persistent automation that needs
the gateway available without a tunnel running on demand:

1. Stand up WireGuard or Tailscale across the bot host and the
   client machines.
2. Bind the gateway to the VPN interface instead of `127.0.0.1`.
   Edit the `mcp-gateway` service's `ports:` block in
   `docker-compose.yml`:

   ```yaml
   ports:
     - "10.0.0.1:9100:9100"   # WireGuard interface IP, for example
   ```

3. Set a bearer token for defence-in-depth — even though the VPN
   is the perimeter, you do not want one device on the VPN to be
   able to take over Discord. In the host shell:

   ```bash
   MCP_GATEWAY_AUTH_TOKEN=$(openssl rand -hex 32)
   ```

   Add it to the gateway's environment in Compose so it survives
   restarts. Distribute the token to clients that need access.

4. Configure clients with the gateway's VPN IP and the bearer
   token:

   ```json
   {
     "mcpServers": {
       "discord": {
         "type": "http",
         "url": "http://10.0.0.1:9100/mcp",
         "headers": {
           "Authorization": "Bearer <token>"
         }
       }
     }
   }
   ```

Tailscale's MagicDNS makes this even easier — you can use the
host's Tailscale name in the URL.

## Pattern 3: Reverse proxy with TLS

For when you really do need a public endpoint (a hosted AI agent,
a multi-team SaaS context, etc.). This is the pattern with the
most operational surface, and the one to think hardest about.

1. Bind the gateway to `127.0.0.1:9100` on the host (the default).
   **Do not** publish it on a public interface directly.
2. Run a reverse proxy (Caddy, nginx, Traefik) in front of it,
   terminating TLS with a real certificate.
3. Set `MCP_GATEWAY_AUTH_TOKEN` to a long random string.
4. Configure the reverse proxy to pass the `Authorization` header
   through unmodified.

Minimal Caddy example:

```caddyfile
mcp.example.com {
    reverse_proxy 127.0.0.1:9100
}
```

Minimal nginx example:

```nginx
server {
    listen 443 ssl;
    server_name mcp.example.com;

    ssl_certificate     /etc/ssl/mcp.example.com.crt;
    ssl_certificate_key /etc/ssl/mcp.example.com.key;

    location / {
        proxy_pass http://127.0.0.1:9100;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
        # Let SSE work
        proxy_buffering off;
        proxy_read_timeout 1h;
    }
}
```

Two configuration details that bite people:

- **MCP uses Server-Sent Events.** The gateway returns long-lived
  SSE responses for every request. nginx's default `proxy_buffering
  on` will hold the whole response until it is finished, which
  breaks streaming. Turn it off (as in the example above), and
  raise `proxy_read_timeout` so it does not kill quiet streams.
- **Set the Host header.** Some proxies default to a host the
  gateway does not expect; pass through the original (`Host
  $host` in nginx, automatic in Caddy) so any future Host-based
  routing in the gateway keeps working.

You should also seriously consider an extra layer of authentication
in front of the gateway — IP allow-listing, mTLS at the proxy,
basic auth on top of the bearer token. The bearer token is a single
secret; one leak (a chat log, a misplaced env file, a CI artifact)
gives an attacker full Discord administrator access via the bot's
account. Defence in depth is appropriate here.

## What MCP can actually do

The MCP catalog is administrative: list / create / delete
channels and roles, ban / kick / timeout members, send messages.
A misconfigured MCP endpoint is, in practical terms, a
ban-everyone-from-your-Discord-and-delete-the-server endpoint.

There is no in-bot confirmation or "are you sure" gate on tool
calls. The bot does whatever the client asks, as long as the bot's
Discord permissions allow it. This is intentional — the entire
point of MCP is to let an AI agent execute server changes without
clicking through prompts — but it means the network and auth layer
is the only thing standing between an attacker and your server.

A few mitigations worth knowing about:

- **The MCP tools are bounded by the bot's Discord permissions.**
  The MCP server cannot do anything the bot itself cannot do. If
  you give the bot only `Manage Roles`, the MCP catalog can only
  manage roles. Audit the bot's role permissions before you expose
  MCP — do not give the bot Administrator unless you really mean
  for the MCP endpoint to have Administrator.
- **Per-call API timeout is 10 seconds** (`API_TIMEOUT` in
  `src/mcp/tools.rs`). A misbehaving client cannot tie up the bot
  with a long-running request.
- **Rate limiting is Discord's, not the bot's.** Bulk operations
  hit Discord's rate limits and surface as errors in the tool
  output. This is not a security feature — it just means an
  attacker cannot bulk-ban 10,000 users in one second.

## Authentication on the bot's MCP, not just the gateway

In a Compose deployment the bot's MCP server binds `0.0.0.0`
inside its container so the gateway sidecar can reach it over the
Docker bridge network. That is a non-loopback bind, so the bot's
own startup check requires `MCP_AUTH_TOKEN` to be set on every
bot.

The model is a **single shared secret** across the whole MCP
fabric: the gateway's `MCP_GATEWAY_AUTH_TOKEN` and every bot's
`MCP_AUTH_TOKEN` must hold the same value. The gateway uses it
for two things at once — it is the bearer it *requires* on
inbound requests from clients, and it is the bearer it *sends*
on outbound requests to each backend bot. Without that match the
backend's Tier-1 auth check rejects the gateway with `401
Unauthorized` and no client can ever reach it. Generate one
value with `openssl rand -hex 32` and paste it everywhere.

If you ever bind a bot's MCP server to a host port directly
(rare, and usually wrong — use the gateway), the same rule
applies: non-loopback means `MCP_AUTH_TOKEN` is required, and the
bot will refuse to start without one.

## Token rotation

Generate a fresh token with `openssl rand -hex 32` (or any
equivalently random source). Because the gateway and every bot
share the same secret, rotation is a flag-day update in three
places:

1. Update `MCP_GATEWAY_AUTH_TOKEN` in the host shell (or in the
   Compose file).
2. Update `MCP_AUTH_TOKEN` in every bot's `.env` to the same
   value.
3. Restart the full stack so the new value takes on both ends:

   ```bash
   docker compose up -d
   ```

Update every MCP client with the new token as well. There is no
built-in multi-token mechanism — rotation is atomic per stack.
Keep the value out of shell history (`export
MCP_GATEWAY_AUTH_TOKEN=$(...)` rather than typing it inline), out
of git, and out of logs.

If you suspect a token leak, rotate immediately, then audit the
bot's recent Discord activity for unexpected actions.

## What not to do

- **Do not publish the gateway on `0.0.0.0:9100` without a token.**
  Even on a "private" network. Networks are less private than they
  look.
- **Do not put the gateway on the internet without TLS.** Bearer
  tokens go in the clear over plain HTTP, and SSE responses leak
  the same token in connection logs along the way.
- **Do not use one bearer token across staging and production.**
  Treat them as separate trust domains.
- **Do not skip the auth token because "it is behind a VPN."**
  Defence in depth costs you nothing here.

## Cross-references

- [MCP Server](../features/mcp-server.md) — what the tools do and
  how clients connect.
- [MCP Gateway Routing](../architecture/mcp-gateway-routing.md) —
  the gateway's internal design.
- [Environment Variables: MCP server](../configuration/environment-variables.md#mcp-server-optional) —
  the variables that control bind, port, and auth.
- [Production Checklist](production-checklist.md) — the one-pass
  hardening sweep that includes MCP.
