# AI Providers

This page documents the `[ai.providers]`, `[ai.routing]`, and `[ai.fallback]` sections of an instance's `config.toml`. All three are optional. An instance with no `[ai.*]` section runs every role on DeepSeek V4.1 Flash at different effort levels, which needs only `DEEPSEEK_API_KEY`. The fallback cascade is off (opt-in).

The baked-in defaults live in [`defaults/providers.toml`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/defaults/providers.toml). That file is compiled into the binary (`include_str!`), so editing it needs a rebuild. Every key in it is also valid in an instance `config.toml` under an `ai.` prefix, so swapping a model is a config edit plus a restart. The one exception is `[aliases]`, which is built-in only.

## Quick reference

```toml
# instances/<your-bot>/config.toml

[ai.providers.<name>]
url = "https://..."           # required: full chat-completions endpoint
model = "..."                 # required: model identifier
api_key_env = "..."           # required: name of env var holding the API key
max_tokens = 8192             # required: per-provider hard cap on response tokens
timeout_secs = 30             # optional, default 30
supports_vision = false       # optional, default false
supports_tools = true         # optional, default true
is_reasoner = false           # optional, default false (a label; see below)
spec = "openai"               # optional, "openai" (default) or "anthropic"
headers = {}                  # optional, extra HTTP headers
auth_header = "Authorization" # optional
auth_scheme = "Bearer "       # optional
extra_body = {}               # optional, merged into every request body
refusal_markers = []          # optional, error-body substrings meaning "filtered"

[ai.routing]
chat = "<provider-name>"      # required if section present
vision = "<provider-name>"    # optional: omitted = image messages handled as text-only
reasoner = "<provider-name>"  # optional: omitted = classifier skipped
fast = "<provider-name>"      # optional: see "Routing degradation rules"

[ai.fallback]
on_censored = ["<name>", "..."]  # ordered fallback cascade (default empty = off)
on_error = false                 # also cascade on HTTP / timeout / credit errors
refusal_check = true             # detect soft (HTTP 200) refusals
refusal_check_max_chars = 1200   # longer replies skip the refusal check
# refusal_check_prompt = "..."   # override the refusal judge's system prompt
# censored_reply = "..."         # reply when everything was filtered
```

## Default registry

When `[ai.providers]` is absent, the bot ships these six definitions (from `defaults/providers.toml`):

| Name | Model | Env var | max_tokens | timeout | vision | tools | `extra_body` | Routed as |
|---|---|---|---|---|---|---|---|---|
| `deepseek_flash` | `deepseek-flash` (V4.1 Flash) | `DEEPSEEK_API_KEY` | 32768 | 90 | yes | yes | `reasoning_effort = "high"` | chat, vision |
| `deepseek_flash_max` | `deepseek-flash` | `DEEPSEEK_API_KEY` | 65536 | 300 | yes | yes | `reasoning_effort = "max"` | reasoner |
| `deepseek_flash_fast` | `deepseek-flash` | `DEEPSEEK_API_KEY` | 4096 | 30 | yes | yes | `thinking = { type = "disabled" }` | fast |
| `deepseek_pro` | `deepseek-v4-pro` (V4-Pro) | `DEEPSEEK_API_KEY` | 65536 | 300 | no | **no** | none | not routed (opt-in) |
| `gemini_flash` | `gemini-3.8-flash` | `GEMINI_API_KEY` | 32768 | 60 | yes | yes | none | not routed |
| `grok` | `grok-4.7` | `GROK_API_KEY` | 32768 | 90 | yes | yes | none | not routed |

All four DeepSeek entries use `https://api.deepseek.com/chat/completions` and carry `refusal_markers = ["Content Exists Risk"]`. `deepseek_flash_max` and `deepseek_pro` set `is_reasoner = true`. Gemini uses Google's OpenAI-compatible endpoint (`https://generativelanguage.googleapis.com/v1beta/openai/chat/completions`). Grok uses `https://api.x.ai/v1/chat/completions`.

`DEEPSEEK_API_KEY` alone covers every default role. `GEMINI_API_KEY` and `GROK_API_KEY` are optional. They are useful as [fallback cascade](#fallback-cascade-aifallback) members, or as role targets if you route to them yourself.

The `deepseek_flash` cap is 32768 because thinking tokens count against `max_tokens`. A smaller cap can run out during reasoning and return no visible output. The bot reports that case as an explicit error (`finish_reason=length`) rather than sending an empty reply.

A provider whose `api_key_env` resolves to an unset or empty env var is "unavailable": defined but not usable. The bot starts and runs without it. AI features that depend on it are disabled, and the bot warns at startup if routing or fallback references it.

## Roles and default routing

| Role | What it does | Default |
|---|---|---|
| `chat` | Default text model. Gets the tools (search, music, moderation, stocks, games). | `deepseek_flash` |
| `vision` | Used when the message, or the message it replies to, has image attachments. | `deepseek_flash` |
| `reasoner` | Used when the `fast` classifier flags a message as needing deep reasoning (math, code, analysis). A tool-capable reasoner runs the normal tool loop. A tool-less one gets web searches done for it first by `fast`. | `deepseek_flash_max` |
| `fast` | Cheap, no-thinking model for one-word utility calls: the reasoner classifier and the soft-refusal judge. It also runs the pre-flight searches for a tool-less reasoner. | `deepseek_flash_fast` |

When `[ai.routing]` is absent, that matrix applies as-is:

```toml
chat = "deepseek_flash"
vision = "deepseek_flash"
reasoner = "deepseek_flash_max"
fast = "deepseek_flash_fast"
```

Pre-0.19 configs sometimes override a default by its old name. When `[ai.routing]` is absent, an `[ai.providers.deepseek_chat]` block (or any other alias of `deepseek_flash`) is routed as **chat**. Likewise, an `[ai.providers.deepseek_reasoner]` block (or another alias of `deepseek_flash_max`) is routed as **reasoner**. For the chat case, startup logs a line saying so. Vision and fast keep their defaults in this case. These overrides are used exactly as written, so they don't pick up the new model or effort settings.

The startup log prints the resolved stack once, so you can confirm what is live:

```
AI routing: chat=deepseek_flash vision=deepseek_flash reasoner=deepseek_flash_max fast=deepseek_flash_fast fallback=[] on_error=false refusal_check=true
```

## Routing degradation rules

When `[ai.routing]` IS present, only the keys you write take effect. There's no field-merge with the defaults above:

| Role | If set | If omitted |
|---|---|---|
| `chat` | Must resolve to a defined provider or an alias, otherwise the bot panics at startup | **Required**: the bot panics at startup |
| `vision` | Must resolve | Vision disabled. Images are dropped and the message text goes to `chat` |
| `reasoner` | Must resolve | Reasoner disabled. The classifier call is skipped and every request goes to `chat` |
| `fast` | Must resolve | `deepseek_flash_fast` if `chat` uses the same `api_key_env` (i.e. chat is on DeepSeek), otherwise `chat` itself |

The `fast` rule exists because a thinking model on a 10-token classifier call spends the whole budget on reasoning and returns nothing. If you route chat to a non-DeepSeek model, utility calls stay on that model rather than being sent to a different vendor. If the routed `fast` provider is unavailable, the bot also falls back to `chat`.

A one-model config:

```toml
[ai.providers.my_local]
url = "http://localhost:11434/v1/chat/completions"
model = "llama3.1:70b"
api_key_env = "LOCAL_LLM_KEY"
max_tokens = 8192

[ai.routing]
chat = "my_local"
# vision, reasoner omitted → disabled; fast → chat (different api_key_env)
```

## Opting into V4-Pro / tuning effort

By default the reasoner is V4.1 Flash at `reasoning_effort = "max"`. That model is multimodal and tool-capable, and it stands in for the older text-only V4-Pro. `deepseek_pro` (V4-Pro) is still defined but not routed.

**Use V4-Pro as the reasoner.** Remember that `[ai.routing]` is all-or-nothing, so restate the other roles:

```toml
[ai.routing]
chat = "deepseek_flash"
vision = "deepseek_flash"
reasoner = "deepseek_pro"
fast = "deepseek_flash_fast"
```

`deepseek_pro` has `supports_tools = false`. Reasoning questions routed to it get a pre-flight search pass on the `fast` provider first, and the results are injected as context (see [AI Pipeline](../architecture/ai-pipeline.md#provider-selection)).

**Turn the reasoner off** by setting `[ai.routing]` and omitting `reasoner`. The classifier call is then skipped too:

```toml
[ai.routing]
chat = "deepseek_flash"
vision = "deepseek_flash"
fast = "deepseek_flash_fast"
# reasoner intentionally omitted
```

**Change the effort level of a default** by redefining it under the same name. The definition is replaced as a whole (no field merge), so copy every field from `defaults/providers.toml` and change the one you care about:

```toml
[ai.providers.deepseek_flash_max]
url = "https://api.deepseek.com/chat/completions"
model = "deepseek-flash"
api_key_env = "DEEPSEEK_API_KEY"
max_tokens = 65536
timeout_secs = 300
supports_vision = true
is_reasoner = true
refusal_markers = ["Content Exists Risk"]
extra_body = { reasoning_effort = "high" }   # was "max"
```

No `[ai.routing]` change is needed, because the default matrix already points `reasoner` at `deepseek_flash_max`.

## Provider definitions

Each `[ai.providers.<name>]` block is independent. The `<name>` is your handle for the provider. It is used in `[ai.routing]`, in `[ai.fallback] on_censored`, and in log lines.

### Required fields

- `url`: full HTTPS endpoint (the chat-completions URL, including any version path).
- `model`: model identifier the provider expects in the request body.
- `api_key_env`: name of an environment variable. The bot reads it at startup. An unset or empty value marks the provider unavailable.
- `max_tokens`: per-provider hard cap on response tokens. The orchestration layer asks for whatever budget it wants and is clamped to this cap. For thinking models, reasoning tokens count against it.

### Optional fields

- `timeout_secs` (default `30`): HTTP timeout for completion calls. Max-effort reasoning needs a long budget (the defaults use `300`).
- `supports_vision` (default `false`): whether the model accepts image content parts. Only vision-capable members are used when an image request cascades.
- `supports_tools` (default `true`): whether the model accepts a `tools` array. This is the **only** switch that controls tool use. If it is `false`, no tools are sent. A tool-less reasoner gets the pre-flight search pass instead.
- `is_reasoner` (default `false`): marks a slow reasoning model. Since 0.19.0 it is a label only. It no longer strips tools, and it is used for startup sanity warnings. Startup warns if a **user-defined** reasoner sets `is_reasoner = true` with `supports_tools = true`, because before 0.19 that combination silently ran without tools. Set `supports_tools = false` if the model rejects tools.
- `spec` (default `"openai"`): request/response shape. `"openai"` and `"anthropic"` are supported. See [Anthropic spec](#anthropic-spec).
- `headers`, `auth_header`, `auth_scheme`: custom HTTP headers and auth. See [Anthropic spec](#anthropic-spec), but they work on any provider.
- `extra_body`: a table merged verbatim into every request body after the standard `model` / `messages` / `max_tokens` / `tools` fields, so it can override those too. Top-level keys overwrite, and nested tables are replaced whole rather than deep-merged. This is the knob for vendor-specific options:

  ```toml
  # DeepSeek: effort level
  extra_body = { reasoning_effort = "max" }

  # DeepSeek: thinking off (fast one-word answers)
  extra_body = { thinking = { type = "disabled" } }
  ```

  Applies to both the OpenAI and Anthropic specs.
- `refusal_markers`: substrings that, found in a **non-2xx** error body, mean the provider's content filter blocked the request. A match is treated as "filtered" (the internal `CENSORED` sentinel), which triggers the [fallback cascade](#fallback-cascade-aifallback):

  ```toml
  refusal_markers = ["Content Exists Risk", "blocked by safety policy"]
  ```

  DeepSeek's `Content Exists Risk` is always matched, even when this list is empty, so user-defined DeepSeek providers keep working.

#### What counts as "filtered"

Any of these surfaces as `CENSORED` and can be redirected by `[ai.fallback] on_censored`:

- A non-2xx error body containing `Content Exists Risk` or any of the provider's `refusal_markers`.
- `finish_reason = "content_filter"` on an OpenAI-spec response.
- `stop_reason = "refusal"` on an Anthropic-spec response.

Separately, a soft refusal (an HTTP 200 reply that dodges the question) is caught by the [refusal check](#soft-refusal-check).

An empty response with `finish_reason = "length"` is **not** filtered. It is an error ("hit max_tokens before producing output"), which usually means `max_tokens` is too small for a thinking model.

### Provider name rules

- Non-empty after `.trim()`
- No internal whitespace characters
- TOML's bare-key rules already constrain `[ai.providers.<name>]` syntax to safe characters (alphanumeric, underscore, hyphen)
- A user-defined name that matches a default-registry name (e.g. `deepseek_flash`) **fully replaces** the default, with no field-level merge. Restate `extra_body` and `refusal_markers` if you still want them.
- A provider defined under an alias name (e.g. `[ai.providers.gemini]`) takes precedence over the alias.

### Backward-compatible alias names

Old and shorthand names still resolve, so existing configs keep working across model migrations. Aliases are built-in only (`defaults/providers.toml` `[aliases]`). To add a new name, define `[ai.providers.<name>]`.

| Alias | Resolves to |
|---|---|
| `deepseek` | `deepseek_flash` |
| `deepseek_chat` | `deepseek_flash` (pre-0.19 canonical name) |
| `deepseek-chat` | `deepseek_flash` |
| `deepseek-flash` | `deepseek_flash` |
| `deepseek-v4` | `deepseek_flash` |
| `deepseek-v4-flash` | `deepseek_flash` |
| `deepseek_reasoner` | `deepseek_flash_max` (pre-0.19 canonical name) |
| `deepseek-reasoner` | `deepseek_flash_max` |
| `deepseek-v4-pro` | `deepseek_pro` |
| `gemini` | `gemini_flash` |
| `xai` | `grok` |

Aliases are accepted everywhere a provider is named: `[ai.routing]` (including startup validation, since 0.19.0) and `[ai.fallback] on_censored`. A cascade that names the same provider twice through different aliases is de-duplicated.

New configs should use the canonical names: `deepseek_flash`, `deepseek_flash_max`, `deepseek_flash_fast`, `deepseek_pro`, `gemini_flash`, `grok`.

### Anthropic spec

As of 0.16.0, `spec = "anthropic"` enables native Anthropic `/v1/messages` routing. This is useful for using Claude directly without going through an OpenAI-compat proxy. Native routing preserves Claude's structured tool use and vision content parts.

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

#### Header and auth fields (also available to OpenAI providers)

- `headers`: `HashMap<String, String>` of extra HTTP headers. Default empty. Values must be printable ASCII. Use inline-table syntax (`headers = { "x" = "y" }`) for 1-2 headers, or a sub-table (`[ai.providers.claude.headers]`) for longer lists.
- `auth_header`: name of the auth header. Default `"Authorization"`. Must be non-empty.
- `auth_scheme`: prefix prepended to the API key. Default `"Bearer "` (with trailing space). Use `""` for Anthropic.

Both the OpenAI and Anthropic paths respect these fields. You can use them on any provider that needs custom auth or headers, such as a self-hosted endpoint that requires a custom `x-internal-auth` header.

#### Translation: what the bot handles automatically

When you route to an Anthropic provider, the bot translates every shape difference transparently. The internal tool definitions, system prompt, and message history are all built in OpenAI shape (the bot's internal canonical form). They are translated to Anthropic's wire shape on each request, so you never need to write Anthropic-specific prompt logic.

Translation covers:

- System prompt → top-level `system` field on the request body (not a `role: "system"` message in the array)
- Image content parts → base64 `source` blocks with correct `media_type`
- Tool definitions → flat `{name, description, input_schema}` shape
- Tool call responses → `tool_use` content blocks are extracted into the same flat `ToolCall { id, name, arguments }` shape the bot uses internally
- Tool result messages → wrapped in user-content `tool_result` blocks
- `stop_reason = "refusal"` → treated as filtered, so it can cascade

#### What works with Claude today

- Text chat (any routing role)
- Vision (when `supports_vision = true`)
- Tool use (when `supports_tools = true`)
- As a fallback cascade member (`[ai.fallback] on_censored = ["claude", ...]`)
- Mixed setups: DeepSeek primary with Claude as a cascade member, Claude primary with DeepSeek as reasoner, and so on

#### What's not yet available

- Streaming responses
- Anthropic's `cache_control` ephemeral blocks for prompt caching
- Structured "thinking" / extended reasoning outputs (Claude's reasoning models)

These are tracked as future enhancements. Today's integration matches DeepSeek and Gemini for the bot's standard workflow.

## Fallback cascade (`[ai.fallback]`)

`[ai.fallback]` controls what happens when the provider that was asked can't or won't answer. The cascade itself is opt-in: with `on_censored` empty (the default), nothing is redirected. A filtered request then gets the canned `censored_reply`, and the soft-refusal check doesn't run.

| Key | Default | Meaning |
|---|---|---|
| `on_censored` | `[]` | Ordered provider names (aliases OK) to retry through when the primary's answer is filtered. The first success wins. The provider that failed is skipped. Duplicates are removed. |
| `on_error` | `false` | Also walk the cascade when the primary fails for a non-content reason (HTTP error, timeout, out of credits). With `false`, those errors produce the usual "Something went wrong" reply. |
| `refusal_check` | `true` | Detect soft refusals and retry them through the cascade. See below. Only runs when `on_censored` has at least one available provider other than the one that answered. |
| `refusal_check_max_chars` | `1200` | Replies longer than this many characters skip the refusal check. Refusals are short, and long answers aren't worth a second call. |
| `refusal_check_prompt` | built-in | Override the refusal judge's system prompt. It must still ask the model to answer only "yes" (refusal) or "no". |
| `censored_reply` | DeepSeek quip | Reply sent when the content filter blocks a request and no cascade member could answer. Change it if your chat model isn't DeepSeek. |

### Soft-refusal check

DeepSeek now usually filters sensitive topics with an HTTP 200 in-character dodge rather than the `Content Exists Risk` error. The wording varies every time, so substring matching can't catch it. When `refusal_check` is on and the cascade is non-empty:

1. A final reply that is short (≤ `refusal_check_max_chars`) and carries no action tool calls goes to the `fast` provider. The bot sends one yes/no call with the user message and the reply.
2. If the judge says "yes", the conversation is retried through `on_censored` in order, skipping the provider that refused. The retry runs without tools.
3. The first non-empty alternative replaces the reply. If the judge errors, or no alternative produces text, the original reply is kept, so the check never blocks a response.

The built-in judge prompt counts a reply as **not** a refusal when it declines something genuinely harmful, or when the message was just small talk. That keeps the cascade from routing around the bot's own safety rules.

### Images

Requests with image attachments use the same cascade, restricted to its vision-capable members (`supports_vision = true`). The soft-refusal check applies there too. If the vision call fails and nothing in the cascade can answer, the bot strips the images and falls through to the text path. The exception is a plain error (not filtered) when chat and vision are the same provider and the cascade is empty: there the bot replies with an error instead of repeating the same request.

### Example: redirect filtered answers to Grok / Gemini

```toml
# .env: DEEPSEEK_API_KEY=..., GROK_API_KEY=..., GEMINI_API_KEY=...

[ai.fallback]
on_censored = ["grok", "gemini_flash"]   # tried in this order
on_error = true                          # also fail over on outages / no credits
# refusal_check = true                   # default: catch in-character dodges too
# refusal_check_max_chars = 1200
# censored_reply = "Can't talk about that one, sorry."
```

Both default Grok and Gemini entries are vision-capable, so image requests can fail over as well. Names whose API key isn't set are skipped with a startup warning.

## Validation behaviour

Performed once at startup, before the bot connects to Discord:

| Case | Behaviour |
|---|---|
| `[ai.routing]` section present without `chat` | Panic |
| `[ai.routing]` `chat` / `vision` / `reasoner` / `fast` names something that is neither a defined provider nor an alias | Panic (the message lists the configured names) |
| Provider name contains whitespace, or bad `headers` / `auth_header` | Panic |
| Provider's `api_key_env` resolves to unset env var | Provider marked unavailable |
| Routing or fallback references an unavailable provider | Warn at startup. The implicitly chosen `fast` is not warned about, because it falls back to chat |
| `vision` role references a provider with `supports_vision = false` | Warn at startup |
| `reasoner` role references a provider with `is_reasoner = false` | Warn at startup |
| User-defined reasoner with `is_reasoner = true` and `supports_tools = true` | Warn at startup (tools are no longer stripped; set `supports_tools = false` if the model rejects them) |
| `[ai.fallback] on_censored` names an unknown or unavailable provider | Warn at startup. The name is skipped in the cascade |

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

Vision and reasoner are disabled. Image attachments are dropped and the text goes to chat. The classifier is skipped, so every prompt goes straight to chat. `fast` resolves to `local` because its `api_key_env` differs from DeepSeek's.

### Mixed providers + cascade

```toml
[ai.providers.openai_gpt]
url = "https://api.openai.com/v1/chat/completions"
model = "gpt-4o"
api_key_env = "OPENAI_API_KEY"
max_tokens = 16384
supports_vision = true

[ai.routing]
chat = "openai_gpt"
vision = "openai_gpt"             # reuse same provider for vision
reasoner = "deepseek_flash_max"   # keep the default reasoner
fast = "deepseek_flash_fast"      # cheap classifier / judge (else it would be chat)

[ai.fallback]
on_censored = ["grok", "gemini_flash"]
```

### Override a default

To keep the default routing but change the chat model's effort level, redefine `deepseek_flash` in full:

```toml
[ai.providers.deepseek_flash]
url = "https://api.deepseek.com/chat/completions"
model = "deepseek-flash"
api_key_env = "DEEPSEEK_API_KEY"
max_tokens = 32768
timeout_secs = 90
supports_vision = true
refusal_markers = ["Content Exists Risk"]
extra_body = { reasoning_effort = "max" }   # default is "high"
```

The user definition fully replaces the default `deepseek_flash` entry. No `[ai.routing]` change is needed, because the default matrix already routes chat and vision to it.

## See also

- [Environment variables](environment-variables.md): `*_API_KEY` reference
- [Instance config](instance-config.md): full `config.toml` schema
- [`defaults/providers.toml`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/defaults/providers.toml): the baked-in defaults (providers, routing, aliases)
- [`defaults/example-providers.toml`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/defaults/example-providers.toml): copy-paste catalogue for popular endpoints
- [AI Chat feature page](../features/ai-chat.md)
- [AI Pipeline](../architecture/ai-pipeline.md): where each role and the cascade fit in the request flow
- Issue [#28](https://github.com/MrMcEpic/discord-bot-rs/issues/28): design context + phase 2 follow-up
