# syntax=docker/dockerfile:1
FROM rust:bookworm AS builder
RUN apt-get update && apt-get install -y cmake libopus-dev libsodium-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry     --mount=type=cache,target=/app/target     cargo build --release && cp target/release/discord-bot /usr/local/bin/discord-bot

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y     ca-certificates     curl     libssl3     libopus0     libsodium23     python3     python3-pip     ffmpeg     && apt-get clean && rm -rf /var/lib/apt/lists/*     && pip3 install --break-system-packages yt-dlp     && curl -fsSL https://deb.nodesource.com/setup_20.x | bash -     && apt-get install -y nodejs     && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/discord-bot /usr/local/bin/discord-bot
WORKDIR /config
CMD ["discord-bot"]
