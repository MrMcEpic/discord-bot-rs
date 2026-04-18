# AI Providers

This page documents the `[ai.providers]`, `[ai.routing]`, and `[ai.fallback]` sections of an instance's `config.toml`. All three are optional — an instance with no `[ai.*]` section uses a sensible default stack (DeepSeek for chat, Gemini for vision, DeepSeek Reasoner for hard questions, no CENSORED cascade).

## Quick reference

```toml
# instances/<your-bot>/config.toml

[ai.providers.<name>]
url = "https://..."           # required: full chat-completions endpoint
model = "..."                 # required: model identifier
api_key_env = "..."           # required: name of env var holding the bearer token
max_tokens = 8192             # required: per-provider hard cap on response tokens
timeout_secs = 30             # optional, default 30
supports_vision = false       # optional, default false
supports_tools = true         # optional, default true
is_reasoner = false           # optional, default false
spec = "openai"               # optional, default "openai"

[ai.routing]
chat = "<provider-name>"      # required if section present
vision = "<provider-name>"    # optional — if omitted, image messages fall through to chat
reasoner = "<provider-name>"  # optional — if omitted, classifier step is skipped

[ai.fallback]
on_censored = ["<name>", "..."]  # CENSORED-cascade chain (optional)
```

## Default registry

When `[ai.providers]` is absent, the bot ships these four definitions:

| Name | URL | Model | Env var | max_tokens | timeout | vision | tools | reasoner |
|---|---|---|---|---|---|---|---|---|
| `deepseek_chat` | `https://api.deepseek.com/chat/completions` | `deepseek-chat` | `DEEPSEEK_API_KEY` | 8192 | 30 | no | yes | no |
| `deepseek_reasoner` | `https://api.deepseek.com/chat/completions` | `deepseek-reasoner` | `DEEPSEEK_API_KEY` | 32768 | 300 | no | no | yes |
| `gemini_flash` | `https://generativelanguage.googleapis.com/v1beta/openai/chat/completions` | `gemini-3-flash-preview` | `GEMINI_API_KEY` | 16384 | 30 | yes | yes | no |
| `grok` | `https://api.x.ai/v1/chat/completions` | `grok-3` | `GROK_API_KEY` | 16384 | 30 | no | yes | no |

A provider whose `api_key_env` resolves to an unset/empty env var is "unavailable" — defined but not usable. The bot starts and runs without it; AI features that depend on it are silently disabled (with a warning at startup if anything references it).

## Default routing

When `[ai.routing]` is absent:

```
chat = "deepseek_chat"
vision = "gemini_flash"
reasoner = "deepseek_reasoner"
```

This is exactly the routing behaviour of every release before 0.15.0. Existing instances pick it up automatically with no config changes.

## Routing degradation rules

When `[ai.routing]` IS present, only the keys you write take effect. There's no field-merge with the defaults above. Specifically:

| Role | If set | If omitted |
|---|---|---|
| `chat` | Must resolve to a configured provider; otherwise the bot panics at startup | **Required** — bot panics at startup |
| `vision` | Must resolve | Image-bearing requests fall through to `chat` with a warning log |
| `reasoner` | Must resolve | Classifier step is skipped; every request goes to `chat` |

This lets you write a one-model config:

```toml
[ai.providers.my_local]
url = "http://localhost:11434/v1/chat/completions"
model = "llama3.1:70b"
api_key_env = "LOCAL_LLM_KEY"
max_tokens = 8192

[ai.routing]
chat = "my_local"
# vision and reasoner omitted → graceful degrade
```

## Provider definitions

Each `[ai.providers.<name>]` block is independent. The `<name>` is your handle for the provider — used in `[ai.routing]` lookups, in `[ai.fallback] on_censored` lists, and in log lines.

### Required fields

- `url` — full HTTPS endpoint (the chat-completions URL, including any version path).
- `model` — model identifier the provider expects in the request body.
- `api_key_env` — name of an environment variable. The bot reads `std::env::var(api_key_env)` at startup; an unset/empty value marks the provider unavailable.
- `max_tokens` — per-provider hard cap on response tokens. The orchestration layer asks for whatever budget it wants and clamps to this cap.

### Optional fields

- `timeout_secs` (default `30`) — HTTP timeout for chat-completions calls. DeepSeek Reasoner needs `300` (5 minutes); fast chat models are fine with the default.
- `supports_vision` (default `false`) — whether the model accepts image content parts. Today only the Gemini default registry entry sets this to `true`.
- `supports_tools` (default `true`) — whether the model accepts a `tools` array. DeepSeek Reasoner is the standout exception (set to `false`).
- `is_reasoner` (default `false`) — flags a slow reasoning model. The orchestration layer uses this signal alongside the longer `timeout_secs` budget you should also set.
- `spec` (default `"openai"`) — request/response shape. `"openai"` (default) and `"anthropic"` (added in 0.16.0) are supported. See [Anthropic spec](#anthropic-spec) for details.

### Provider name rules

- Non-empty after `.trim()`
- No internal whitespace characters
- TOML's bare-key rules already constrain `[ai.providers.<name>]` syntax to safe characters (alphanumeric, underscore, hyphen, dot)
- A user-defined name that matches a default-registry name (e.g. `gemini_flash`) **fully replaces** the default — no field-level merge

### Backward-compatible alias names

For instance configs written before 0.15.0, three short aliases are accepted at runtime when looking up providers by name:

| Alias | Resolves to |
|---|---|
| `gemini` | `gemini_flash` |
| `deepseek` | `deepseek_chat` |
| `deepseek-chat` | `deepseek_chat` |

These aliases work in `[ai.fallback] on_censored` and in any other place the bot looks up a provider by name at request time. They are **not** accepted by `[ai.routing]` startup validation — using `[ai.routing] chat = "gemini"` panics at startup with the canonical name (`gemini_flash`) in the error message.

New configs should use the canonical names. The aliases exist purely so a `[ai.fallback] on_censored = ["grok", "gemini"]` line copied from a 0.14.0 example doesn't silently produce an empty cascade.

### Anthropic spec

As of 0.16.0, `spec = "anthropic"` enables native Anthropic `/v1/messages` routing. This is useful for using Claude directly without going through an OpenAI-compat proxy — native routing preserves Claude's structured tool use, vision content parts, and prompt caching (future work).

An Anthropic provider definition looks like this:

```toml
[ai.providers.claude]
spec = "anthropic"
url = "https://api.anthropic.com/v1/messages"
model = "claude-opus-4-7"
api_key_env = "ANTHROPIC_API_KEY"
max_tokens = 8192
supports_vision = true
supports_tools = true

# Anthropic's auth is x-api-key with no scheme prefix.
auth_header = "x-api-key"
auth_scheme = ""

# Anthropic's required version header.
headers = { "anthropic-version" = "2023-06-01" }
```

#### New fields (also available to OpenAI providers)

- `headers` — `HashMap<String, String>` of extra HTTP headers. Default empty. Values must be printable ASCII. Use inline-table syntax (`headers = { "x" = "y" }`) for 1-2 headers, or a sub-table (`[ai.providers.claude.headers]`) for longer lists.
- `auth_header` — name of the auth header. Default `"Authorization"`. Must be non-empty.
- `auth_scheme` — prefix prepended to the API key. Default `"Bearer "` (with trailing space). Use `""` for Anthropic.

These fields are respected by both the OpenAI and Anthropic paths — you can use them on any provider that needs custom auth or headers (e.g. a self-hosted endpoint requiring a custom `x-internal-auth` header).

#### Translation — what the bot handles automatically

When you route to an Anthropic provider, the bot translates every shape difference transparently. The internal tool definitions, system prompt, and message history are all built in OpenAI shape (the bot's internal canonical form) and translated to Anthropic's wire shape on each request. You never need to write Anthropic-specific prompt logic.

Translation covers:

- System prompt → top-level `system` field on the request body (not a `role: "system"` message in the array)
- Image content parts → base64 `source` blocks with correct `media_type`
- Tool definitions → flat `{name, description, input_schema}` shape
- Tool call responses → `tool_use` content blocks are extracted into the same flat `ToolCall { id, name, arguments }` shape the bot uses internally
- Tool result messages → wrapped in user-content `tool_result` blocks

#### What works with Claude today

- Text chat (any routing role)
- Vision (when `supports_vision = true`)
- Tool use (when `supports_tools = true`)
- Multi-round search via the CENSORED cascade / `[ai.fallback] on_censored = ["claude", ...]` as a post-DeepSeek-refusal fallback
- Mixed setups: DeepSeek primary + Claude as cascade member, or Claude primary + DeepSeek as reasoner, etc.

#### What's not yet available

- Streaming responses
- Anthropic's `cache_control` ephemeral blocks for prompt caching
- Structured "thinking" / extended reasoning outputs (Claude's reasoning models)

These are tracked as future enhancements; today's integration gives feature parity with DeepSeek/Gemini for the bot's standard workflow.

## Validation behaviour

Performed once at startup, before the bot connects to Discord:

| Case | Behaviour |
|---|---|
| `[ai.routing] chat` unset OR points at unknown provider name | Panic |
| `[ai.routing] vision` / `reasoner` set to unknown provider name | Panic |
| `[ai.routing]` section present without `chat` | Panic |
| Provider name contains whitespace | Panic |
| Provider with `spec = "anthropic"` | Fully supported as of 0.16.0 — see [Anthropic spec](#anthropic-spec) |
| Provider's `api_key_env` resolves to unset env var | Provider marked unavailable |
| Routing or fallback references an unavailable provider | Warn at startup |
| `[ai.routing] vision` references provider with `supports_vision = false` | Warn at startup |
| `[ai.routing] reasoner` references provider with `is_reasoner = false` | Warn at startup |
| `[ai.fallback] on_censored` references defined-but-unavailable provider | Warn at startup; `cascade_for` skips it at request time |
| `[ai.fallback] on_censored` references completely unknown name | Warn at request time only (via `cascade_for`); silently produces empty cascade entry. Common when migrating from 0.14.0 configs that used aliases — see [Backward-compatible alias names](#backward-compatible-alias-names) above |

## Worked examples

### One-model setup (local Ollama)

```toml
[ai.providers.local]
url = "http://localhost:11434/v1/chat/completions"
model = "llama3.1:70b"
api_key_env = "OLLAMA_API_KEY"   # any non-empty value works for Ollama
max_tokens = 8192

[ai.routing]
chat = "local"
```

Vision and reasoner gracefully degrade. Image messages will be handed to the local model anyway; classifier is skipped so every prompt goes straight to chat.

### Three providers + cascade

```toml
[ai.providers.openai_gpt]
url = "https://api.openai.com/v1/chat/completions"
model = "gpt-4o"
api_key_env = "OPENAI_API_KEY"
max_tokens = 16384
supports_vision = true

[ai.routing]
chat = "openai_gpt"
vision = "openai_gpt"            # reuse same provider for vision
reasoner = "deepseek_reasoner"   # keep the default reasoner

[ai.fallback]
on_censored = ["grok", "gemini_flash"]
```

### Override a default

To use `gemini_flash` but with a different model:

```toml
[ai.providers.gemini_flash]
url = "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
model = "gemini-2.5-pro"           # not the default flash; check ai.google.dev for newer Pro models
api_key_env = "GEMINI_API_KEY"
max_tokens = 32768
supports_vision = true
```

The user definition fully replaces the default `gemini_flash` entry. No `[ai.routing]` change needed if you're keeping the same role assignment.

## See also

- [Environment variables](environment-variables.md) — `*_API_KEY` reference
- [Instance config](instance-config.md) — full `config.toml` schema
- [`defaults/example-providers.toml`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/defaults/example-providers.toml) — copy-paste catalogue for popular endpoints
- [AI Chat feature page](../features/ai-chat.md)
- Issue [#28](https://github.com/MrMcEpic/discord-bot-rs/issues/28) — design context + phase 2 follow-up
