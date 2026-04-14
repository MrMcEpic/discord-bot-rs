FROM rust:bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release && \
    cp target/release/mcp-gateway /usr/local/bin/mcp-gateway

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
