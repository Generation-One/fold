# Build stage
FROM rust:latest as builder

WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source
COPY src ./src
COPY migrations ./migrations

# Build release binary (touch main.rs to force rebuild)
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary
COPY --from=builder /app/target/release/fold /app/fold

# Copy migrations
COPY migrations ./migrations

# Create data directories
RUN mkdir -p /data/attachments /data/summaries

# Non-root user
RUN useradd -r -s /bin/false fold && \
    chown -R fold:fold /app /data

USER fold

EXPOSE 8765

ENV HOST=0.0.0.0
ENV PORT=8765
ENV DATABASE_PATH=/data/fold.db
ENV ATTACHMENTS_PATH=/data/attachments
ENV SUMMARIES_PATH=/data/summaries

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8765/health || exit 1

CMD ["/app/fold"]
