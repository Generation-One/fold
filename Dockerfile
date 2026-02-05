# Build stage
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Copy manifests and all crates
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary
RUN cargo build --release -p fold-core

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary
COPY --from=builder /app/target/release/fold-core /app/fold

# Create data directory
RUN mkdir -p /data

# Non-root user
RUN useradd -r -s /bin/false fold && \
    chown -R fold:fold /app /data

USER fold

EXPOSE 8765

ENV HOST=0.0.0.0
ENV PORT=8765
ENV DATABASE_PATH=/data/fold.db
ENV RUST_LOG=fold=info,tower_http=info

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8765/health || exit 1

CMD ["/app/fold"]
