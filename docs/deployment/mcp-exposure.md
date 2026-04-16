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

## The default: localhost only

The defaults are deliberately safe:

- The bot's own MCP server defaults to `MCP_BIND_ADDR=127.0.0.1`
  and `MCP_PORT=9090`. Inside the Compose stack, each bot
  container has its own loopback, so this means "reachable from
  inside the bot's container only," and the gateway reaches the
  bots over the Docker bridge network using each bot's service
  name (`http://bot:9090`). Nothing on the host can hit the bot's
  MCP port directly.
- The gateway publishes `127.0.0.1:9100` on the host. That means
  it is reachable from any process running on the host (an
  `ssh`-ed user, a process started by your shell), but **not** from
  any other machine on the network and not from the public
  internet.
- `MCP_AUTH_TOKEN` and `MCP_GATEWAY_AUTH_TOKEN` both default to
  empty, which disables auth — fine on a loopback address, fatal
  anywhere else.

A fresh `docker compose up -d` therefore exposes nothing to the
internet. The MCP endpoint is available to a Claude Code on the
same host (`http://localhost:9100/mcp`), to anyone you `ssh -L
9100:localhost:9100` to, and to nothing else.

If that covers your use case, **stop here**. The default is the
right answer for most operators. The rest of this page is about
the cases where it is not.

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

In a default Compose deployment the bots' MCP servers are not
exposed to anything outside the Compose network, so leaving
`MCP_AUTH_TOKEN` empty per bot is fine — the gateway is the
authenticated front door and the bots' own ports are private. The
gateway talks to bots over the internal network with no auth.

If you ever bind a bot's MCP server to a host port directly
(rare, and usually wrong — use the gateway), set `MCP_AUTH_TOKEN`
on that bot's `.env` and configure your client with the matching
bearer.

## Token rotation

Generate a fresh token with `openssl rand -hex 32` (or any
equivalently random source). Update `MCP_GATEWAY_AUTH_TOKEN` in
the host shell or in the Compose file, then restart the gateway:

```bash
docker compose up -d mcp-gateway
```

Update every client with the new token. There is no built-in
multi-token mechanism — rotation is a flag day. Keep the value out
of shell history (`export MCP_GATEWAY_AUTH_TOKEN=$(...)` rather
than typing it inline), out of git, and out of logs.

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
