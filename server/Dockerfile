# syntax=docker/dockerfile:1.6
FROM rust:1.89-bookworm AS builder
WORKDIR /src

# System deps needed for build (rustls uses ring, no openssl required).
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config build-essential ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p messenger-server \
 && cp /src/target/release/messenger-server /usr/local/bin/messenger-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates tini sqlite3 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --uid 1001 messenger
WORKDIR /app
COPY --from=builder /usr/local/bin/messenger-server /usr/local/bin/messenger-server
RUN mkdir -p /app/data && chown -R messenger:messenger /app

USER messenger
ENV MESSENGER_BIND_ADDR=0.0.0.0:8080 \
    MESSENGER_DATABASE_URL=sqlite:///app/data/messenger.db?mode=rwc \
    MESSENGER_DATA_DIR=/app/data \
    MESSENGER_LOG_FORMAT=Pretty \
    RUST_LOG=info,messenger=info

EXPOSE 8080
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/messenger-server"]
