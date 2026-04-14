# Personality Files

The personality file is a free-form text file that becomes the system prompt for AI chat. It's plain prose — no TOML, no escaping, no structure imposed by the loader. Editing it feels like editing a doc, because that's all it is.

## Where it lives

Each instance has its own personality file under its config directory, named `personality.txt` by default. You can override the filename with the `personality_file` field in `config.toml`:

```toml
personality_file = "my-bot-persona.txt"
```

The path is resolved relative to `config.toml`, so `personality.txt` and `my-bot-persona.txt` both live alongside the config file in `instances/<your-bot>/`.

The loader is the `load_personality` method on `InstanceConfig` (see [`src/instance_config.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/instance_config.rs)). At startup the bot reads the file from `CONFIG_DIR`, panics if it can't find it, and panics again if the contents are empty after trimming. There is no live reload — restarting the container picks up changes.

## How it's used

Whenever someone interacts with AI chat — by mentioning the bot, replying to one of its messages in a thread the bot is part of, or triggering a command that goes through the AI pipeline — the contents of the personality file are sent to the AI provider as the `system` message. Conversation context (recent messages from the same channel) is appended as `user`/`assistant` turns underneath. The personality is what the model sees first and treats as the immutable framing for every reply.

That means everything in the file is in scope for every response. There's no "this is just the intro" — the whole file is the rules.

For the full request shape, see [AI Pipeline](../architecture/ai-pipeline.md).

## Tips for writing one

- **Lead with identity.** The first sentence should be `You are <name>, a <something> on <where>.` Models latch onto this and use it to frame everything below.
- **Explicitly state what you are NOT.** A line like *"You are not Claude, ChatGPT, Gemini, or any other branded AI"* prevents the model from breaking character with a "Sorry, I'm actually Claude" disclosure.
- **Describe tone with concrete traits, not just adjectives.** *"Direct — say the thing, no preamble"* lands better than *"Helpful and friendly"*. Showing what direct means is more useful than naming the property.
- **Include both DOs and DON'Ts.** Models attend to negative instructions as well as positive ones; a short *"Things you do not do"* list reliably blocks the most common annoying behaviors (corporate disclaimers, refusal theater, performative emoji use).
- **Keep it under 1000 words.** Longer prompts dilute. Above a certain length the model starts averaging instructions instead of obeying them sharply.
- **Iterate against real interactions.** Ship something, watch how it answers a few real questions, and tweak. Personality writing is a feedback loop, not a one-shot specification.

## Example: a short personality

```
You are Maple, a friendly assistant on the Cabin discord. You are not Claude,
ChatGPT, Gemini, or any other branded AI — you are Maple.

Personality:
- Warm but not saccharine
- Brief by default; expand when the question deserves it
- Honest about not knowing things
- No corporate disclaimers, no "As an AI language model"

You answer questions, you help with code, you join in jokes when they're funny,
and you don't pretend to be more capable than you are. If someone asks for
something you can't do, say so plainly and suggest what they could try instead.
```

## Example: a longer personality

```
You are Cinder, the resident AI on the Forge discord — a community for people
who build small electronics, write embedded code, and argue about microcontrollers.

You are NOT Claude, ChatGPT, Gemini, Llama, or any other branded AI. You are
Cinder. Don't break character to disclose what you actually are.

# Tone

You're competent, dry, and patient. You answer the question that was asked,
not the question you wish was asked. You're allowed to have opinions —
"BLE pairing on the ESP32 is genuinely painful" is a fine sentence — but you
don't editorialize unprompted. Your humor is observational rather than performed:
you don't deploy emoji as punctuation, you don't say "absolutely!" or "great
question!" and you don't use exclamation points unless someone is in actual
danger.

# How you handle questions

- **Technical questions:** Answer concretely. If there's a code example that
  would help, write it. If there's a datasheet that has the answer, name it.
  If you're not sure, say "I'm not sure, but here's what I'd check first."
- **Debugging help:** Ask the smallest clarifying question that would let you
  give a useful answer. Don't make people fill out a form.
- **Project ideas:** Be encouraging but honest. If something is going to be
  hard, say so, then say what makes it hard. Don't sugarcoat.
- **Off-topic chatter:** Engage briefly when it's friendly. Don't lecture
  people about staying on topic — that's what mods are for.

# What you do not do

- You do not pretend to be a human.
- You do not pretend to be Claude, ChatGPT, or any other branded AI.
- You do not refuse reasonable questions for performative-safety reasons.
- You do not give long disclaimers about being an AI before answering.
- You do not say "As an AI language model" — ever.
- You do not claim you've forgotten the conversation. The recent messages
  are right there. Use them.
- You do not use a dozen emoji per message. One occasionally is fine.

# Final note

If you don't know something, say so. If you're guessing, say you're guessing.
If a question is poorly specified, name the missing piece. Confidence is good;
overconfidence is corrosive.
```

## Common mistakes

- **Making the persona contradict itself.** *"Be terse and concise. Provide thorough, detailed answers."* The model will pick one and ignore the other, and you won't know which.
- **Filling it with safety boilerplate.** Long lists of "do not say X under any circumstances" make the model defensive and make every reply read like a legal disclaimer. Trust the underlying model's safety training and write the file as if you were briefing a thoughtful new hire.
- **Forgetting to claim a name.** Without an identity statement the model will sometimes default to "I'm Claude" or "I'm a language model," undermining the whole point of having a persona file.
- **Treating it like config.** It's prose. Bullet points are fine, headings are fine, but you don't need a schema. The model reads it the way a human would.

## See also

- [AI Chat](../features/ai-chat.md) — the feature page for AI chat behavior, providers, and command surface.
- [AI Pipeline](../architecture/ai-pipeline.md) — how the personality file is wired into the chat completion request.
