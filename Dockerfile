# syntax=docker/dockerfile:1

# ---- Stage 1: chef base ----
# cargo-chef separates dependency compilation from app compilation. The
# dep-build layer is cached as long as Cargo.toml + Cargo.lock are
# unchanged, so app-only edits skip recompiling all transitive crates.
FROM rust:bookworm AS chef
RUN apt-get update && apt-get install -y cmake libopus-dev libsodium-dev && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /app

# ---- Stage 2: planner ----
# Walks the source tree once and emits recipe.json: a structural summary
# of the workspace (deps, features) with no source code in it. Source
# changes don't change recipe.json unless they touch dep manifests.
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---- Stage 3: builder ----
# `cargo chef cook` compiles only what recipe.json describes — i.e.
# every dep, none of our app. Layer is cached by the recipe.json hash.
# Then we COPY source and run a regular release build, which now only
# has to compile our own crates against the already-built deps.
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && cp target/release/discord-bot /usr/local/bin/discord-bot

# ---- Stage 4: runtime ----
FROM debian:bookworm-slim
LABEL org.opencontainers.image.source="https://github.com/MrMcEpic/discord-bot-rs"
LABEL org.opencontainers.image.description="Multi-instance Discord bot framework written in Rust"
LABEL org.opencontainers.image.licenses="AGPL-3.0-or-later"
LABEL org.opencontainers.image.documentation="https://mrmcepic.github.io/discord-bot-rs"
LABEL org.opencontainers.image.authors="MrMcEpic"
RUN apt-get update && apt-get install -y \
    ca-certificates curl libssl3 libopus0 libsodium23 \
    python3 python3-pip ffmpeg \
    && apt-get clean && rm -rf /var/lib/apt/lists/* \
    && pip3 install --break-system-packages yt-dlp \
    && curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/discord-bot /usr/local/bin/discord-bot
WORKDIR /config
HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=12 \
  CMD curl -sf --connect-timeout 2 http://localhost:9090/mcp || exit 1
CMD ["discord-bot"]
