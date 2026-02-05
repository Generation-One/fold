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

# =============================================================================
# Environment Variables
# =============================================================================
# All configuration is done via environment variables. Pass them at runtime:
#   docker run -e GOOGLE_API_KEY=xxx -e QDRANT_URL=http://qdrant:6334 ...
#
# REQUIRED (at least one embedding provider):
#   GOOGLE_API_KEY        - Gemini API key for embeddings (recommended)
#   OPENAI_API_KEY        - OpenAI API key for embeddings
#   OLLAMA_URL            - Ollama base URL for local embeddings
#
# SERVER:
#   HOST                  - Bind address (default: 0.0.0.0)
#   PORT                  - Listen port (default: 8765)
#   PUBLIC_URL            - Public URL for OAuth callbacks (default: http://localhost:8765)
#   TLS_CERT_PATH         - Path to TLS certificate (optional)
#   TLS_KEY_PATH          - Path to TLS private key (optional)
#
# DATABASE:
#   DATABASE_PATH         - SQLite database path (default: /data/fold.db)
#   QDRANT_URL            - Qdrant vector DB URL (default: http://localhost:6334)
#   QDRANT_COLLECTION_PREFIX - Collection name prefix (default: fold_)
#
# AUTH (optional, for OAuth login):
#   ADMIN_BOOTSTRAP_TOKEN - Initial admin token for first-time setup
#   SESSION_SECRET        - Secret for session cookies (auto-generated if not set)
#   SESSION_MAX_AGE       - Session lifetime in seconds (default: 604800 / 7 days)
#
#   OAuth providers use pattern: AUTH_PROVIDER_{NAME}_{FIELD}
#   Example for GitHub:
#     AUTH_PROVIDER_GITHUB_TYPE=github
#     AUTH_PROVIDER_GITHUB_CLIENT_ID=xxx
#     AUTH_PROVIDER_GITHUB_CLIENT_SECRET=xxx
#
# LLM (optional, for AI features):
#   ANTHROPIC_API_KEY     - Claude API key
#   ANTHROPIC_MODEL       - Model name (default: claude-3-5-haiku-20241022)
#   OPENROUTER_API_KEY    - OpenRouter API key
#   OPENROUTER_MODEL      - Model name (default: meta-llama/llama-3-8b-instruct:free)
#   OPENAI_API_KEY        - OpenAI API key (also used for embeddings)
#   OPENAI_MODEL          - Model name (default: gpt-4o-mini)
#
# EMBEDDINGS:
#   GEMINI_EMBEDDING_MODEL  - Gemini model (default: text-embedding-001)
#   OPENAI_EMBEDDING_MODEL  - OpenAI model (default: text-embedding-3-small)
#   OLLAMA_EMBEDDING_MODEL  - Ollama model (default: nomic-embed-text:latest)
#   EMBEDDING_DIMENSION     - Vector dimension (default: 768)
#
# STORAGE:
#   ATTACHMENTS_PATH      - Path for file attachments (default: /data/attachments)
#   SUMMARIES_PATH        - Path for summaries (default: /data/summaries)
#   MAX_ATTACHMENT_SIZE   - Max upload size in bytes (default: 10485760 / 10MB)
#   FOLD_PATH             - Base path for memory content (default: fold)
#
# INDEXING:
#   INDEXING_CONCURRENCY  - Parallel file indexing limit (default: 4)
#
# LOGGING:
#   RUST_LOG              - Log level (default: fold=info,tower_http=info)
# =============================================================================

ENV HOST=0.0.0.0
ENV PORT=8765
ENV DATABASE_PATH=/data/fold.db
ENV ATTACHMENTS_PATH=/data/attachments
ENV SUMMARIES_PATH=/data/summaries
ENV RUST_LOG=fold=info,tower_http=info

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8765/health || exit 1

CMD ["/app/fold"]
