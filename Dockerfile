# Stage 1: Build
# Dependencies require Rust 1.88+
FROM rust:1.88-slim-bookworm AS builder

WORKDIR /usr/src/overlord

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

# Stage 2: Minimal Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
RUN mkdir -p /app/data

COPY --from=builder /usr/src/overlord/target/release/overlord /app/overlord

ENV DATABASE_URL="sqlite:///app/data/overlord.db?mode=rwc"

VOLUME ["/app/data"]

CMD ["/app/overlord"]

