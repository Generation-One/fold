# Build stage
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./
COPY crates/fold-core/Cargo.toml ./crates/fold-core/
COPY crates/fold-embeddings/Cargo.toml ./crates/fold-embeddings/

# Create dummy sources to build dependencies
RUN mkdir -p crates/fold-core/src crates/fold-embeddings/src && \
    echo "fn main() {}" > crates/fold-core/src/main.rs && \
    echo "pub fn dummy() {}" > crates/fold-embeddings/src/lib.rs && \
    cargo build --release && \
    rm -rf crates/*/src

# Copy actual source (schema is embedded via include_str!)
COPY crates ./crates

# Build release binary (touch main.rs to force rebuild)
RUN touch crates/fold-core/src/main.rs && cargo build --release

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
