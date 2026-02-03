# Build stage
FROM rust:1.83-bookworm as builder

WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source and schema
COPY src ./src
COPY schema.sql ./schema.sql

# Build release binary (touch main.rs to force rebuild)
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary and schema
COPY --from=builder /app/target/release/fold /app/fold
COPY --from=builder /app/schema.sql /app/schema.sql

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
