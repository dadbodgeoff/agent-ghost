# Multi-stage Dockerfile for ghost-gateway binary (Req 40 AC1).

# ── Build stage ──────────────────────────────────────────────────
FROM rust:1.80-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build release binary
RUN cargo build --release --bin ghost \
    && strip target/release/ghost

# ── Runtime stage ────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

# Create ghost user
RUN useradd --create-home --shell /bin/bash ghost

COPY --from=builder /build/target/release/ghost /usr/local/bin/ghost

# Default directories
RUN mkdir -p /home/ghost/.ghost/data \
    /home/ghost/.ghost/config \
    /home/ghost/.ghost/backups \
    && chown -R ghost:ghost /home/ghost/.ghost

USER ghost
WORKDIR /home/ghost

EXPOSE 18789 18790

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ghost status || exit 1

ENTRYPOINT ["ghost"]
CMD ["serve"]
