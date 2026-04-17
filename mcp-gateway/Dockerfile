# syntax=docker/dockerfile:1

# ---- Stage 1: chef base ----
# cargo-chef separates dependency compilation from app compilation so
# app-only edits skip recompiling transitive crates.
FROM rust:bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# ---- Stage 2: planner ----
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---- Stage 3: builder ----
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp target/release/mcp-gateway /usr/local/bin/mcp-gateway

# ---- Stage 4: runtime ----
FROM debian:bookworm-slim
LABEL org.opencontainers.image.source="https://github.com/MrMcEpic/discord-bot-rs"
LABEL org.opencontainers.image.description="MCP gateway for routing MCP requests across discord-bot-rs instances"
LABEL org.opencontainers.image.licenses="AGPL-3.0-or-later"
LABEL org.opencontainers.image.documentation="https://mrmcepic.github.io/discord-bot-rs"
LABEL org.opencontainers.image.authors="MrMcEpic"
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl libssl3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/mcp-gateway /usr/local/bin/mcp-gateway
EXPOSE 9100
CMD ["mcp-gateway"]
