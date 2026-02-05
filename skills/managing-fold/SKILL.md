---
name: managing-fold
description: Use this skill when deploying, configuring, or operating a Fold instance. Triggers on keywords like Fold deployment, Fold configuration, docker-compose setup, Qdrant scaling, LLM provider setup, auth configuration, webhook setup, or production operations for Fold.
---

# Managing Fold

Deploy, configure and operate Fold instances for teams and AI agents.

**Latest Documentation**: Always check [github.com/Generation-One/fold/wiki](https://github.com/Generation-One/fold/wiki) for current deployment guides and configuration options.

# Quick Deployment

## Docker Compose (Recommended)

```bash
# Clone and start
git clone https://github.com/Generation-One/fold.git
cd fold
docker-compose up -d

# Check health
curl http://localhost:8765/health
```

## Known Build Issues

### 1. floor_char_boundary Unstable API (Rust 1.85+)

If you get `floor_char_boundary is unstable` errors, add this helper function to `crates/fold-core/src/services/memory.rs` after the imports:

```rust
/// Helper function to find the largest valid UTF-8 boundary at or before `index`.
/// Replacement for unstable str::floor_char_boundary.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        s.len()
    } else {
        let mut i = index;
        while i > 0 && !s.is_char_boundary(i) {
            i -= 1;
        }
        i
    }
}
```

Then replace any `s.floor_char_boundary(n)` calls with `floor_char_boundary(s, n)`.

### 2. Dockerfile Workspace Structure

The Dockerfile must handle the `crates/` workspace layout. Ensure the builder stage copies the entire crates directory:

```dockerfile
FROM rust:1.85-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p fold-core

FROM debian:bookworm-slim
# ... runtime stage
COPY --from=builder /app/target/release/fold /app/fold
```

## Environment Configuration

Create `.env` file with required variables:

```bash
# Required
DATABASE_URL=sqlite:///data/fold.db
QDRANT_URL=http://qdrant:6334

# LLM Provider API Keys
GOOGLE_API_KEY=your-key         # For Gemini embeddings
OPENROUTER_API_KEY=your-key     # For LLM (Claude, etc.)
OPENAI_API_KEY=your-key         # Alternative LLM

# Authentication (optional - defaults to token auth)
JWT_SECRET=generate-secure-secret
AUTH_PROVIDER=google            # google, github, or oidc
```

**Note**: API keys are stored in the database, not just env vars. See Provider Configuration below.

# Provider Configuration

Providers are configured in SQLite tables `embedding_providers` and `llm_providers`. The `.env` file provides initial keys, but you can manage providers via the database.

## Embedding Providers

Each provider has `priority` (for indexing) and `search_priority` (in config JSON):

```sql
-- View current providers
SELECT name, enabled, priority, config FROM embedding_providers;

-- Add Ollama for local embeddings (high index priority, low search priority)
INSERT INTO embedding_providers (id, name, enabled, priority, auth_type, config)
VALUES (
  'uuid-here',
  'ollama',
  1,
  100,  -- index_priority: prefer for indexing
  'none',
  '{"endpoint":"http://host.docker.internal:11434","model":"nomic-embed-text","search_priority":1}'
);

-- Update Gemini to lower index priority
UPDATE embedding_providers SET priority = 1 WHERE name = 'gemini';
```

**Docker networking for Ollama**: Use your Docker network gateway IP instead of `host.docker.internal`:
```bash
# Find gateway IP
docker network inspect fold_default | jq '.[0].IPAM.Config[0].Gateway'
# Returns something like "172.19.0.1"

# Use in Ollama endpoint
"endpoint":"http://172.19.0.1:11434"
```

## LLM Providers

```sql
-- View current providers
SELECT name, enabled, priority, config FROM llm_providers;

-- Update to use Claude via OpenRouter
UPDATE llm_providers 
SET config = '{"endpoint":"https://openrouter.ai/api/v1","model":"anthropic/claude-3-5-haiku-latest"}'
WHERE name = 'openrouter';
```

After modifying providers, restart Fold to reload configuration.

# UI Deployment

The Fold UI is a separate React app that needs the API URL baked in at build time.

```dockerfile
# UI Dockerfile
FROM node:20-alpine AS builder
WORKDIR /app
ARG VITE_API_URL=https://fold.example.com
ENV VITE_API_URL=${VITE_API_URL}
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build

FROM nginx:alpine
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist /usr/share/nginx/html
EXPOSE 80
```

Build with the correct API URL:
```bash
docker build --build-arg VITE_API_URL=https://fold.example.com -t fold-ui .
```

# Traefik Routing with SSO

For production with Authelia SSO, configure Traefik to bypass auth for API/health/MCP endpoints:

```yaml
# traefik dynamic config
http:
  routers:
    fold-api:
      rule: "Host(`fold.example.com`) && (PathPrefix(`/api`) || PathPrefix(`/health`) || PathPrefix(`/mcp`))"
      service: fold
      entryPoints:
        - websecure
      tls:
        certResolver: myresolver
      # No auth middleware - uses Bearer token auth

    fold-ui:
      rule: "Host(`fold.example.com`)"
      service: fold-ui
      entryPoints:
        - websecure
      tls:
        certResolver: myresolver
      middlewares:
        - authelia@docker  # SSO for UI only

  services:
    fold:
      loadBalancer:
        servers:
          - url: "http://fold:8765"
    fold-ui:
      loadBalancer:
        servers:
          - url: "http://fold-ui:80"
```

# Core Operations

## 1. Project Management

```bash
# Create project
curl -X POST http://localhost:8765/api/projects \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "My Project", "slug": "my-project"}'

# List projects
curl http://localhost:8765/api/projects \
  -H "Authorization: Bearer $TOKEN"
```

## 2. Repository Integration

Connect GitHub repositories for automatic indexing:

```bash
# Add repository to project
curl -X POST http://localhost:8765/api/projects/my-project/repositories \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://github.com/org/repo",
    "branch": "main",
    "access_token": "ghp_xxx"  # GitHub PAT with repo scope
  }'

# Trigger reindex
curl -X POST http://localhost:8765/api/projects/my-project/repositories/{repo_id}/reindex \
  -H "Authorization: Bearer $TOKEN"
```

## 3. API Token Management

```bash
# Create API token
curl -X POST http://localhost:8765/api/tokens \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "mai-api", "scopes": ["read", "write"]}'
```

Token format: `fold_{prefix}_{secret}` - only the SHA256 hash of the full token is stored.

## 4. Health Monitoring

```bash
# Check service health
curl http://localhost:8765/health

# Get full system status
curl http://localhost:8765/status
```

## 5. Index Management

```bash
# Trigger full reindex for a repository
curl -X POST http://localhost:8765/api/projects/my-project/repositories/{repo_id}/reindex \
  -H "Authorization: Bearer $TOKEN"

# Check job status
curl http://localhost:8765/api/jobs/{job_id} \
  -H "Authorization: Bearer $TOKEN"
```

## 6. Backup and Restore

SQLite database at `/data/fold.db` contains all metadata. Qdrant stores vectors at `qdrant_data` volume.

```bash
# Backup database
docker exec fold-fold-1 sqlite3 /data/fold.db ".backup /data/backup.db"

# Copy backup out
docker cp fold-fold-1:/data/backup.db ./fold-backup-$(date +%Y%m%d).db

# Backup Qdrant
curl -X POST http://localhost:6333/collections/fold_memories/snapshots
```

# Troubleshooting

## Container fails to start

```bash
# Check logs
docker logs fold-fold-1

# Common issues:
# - Missing env vars
# - Qdrant not accessible
# - Database permissions
```

## Reindex fails with "Project root path does not exist"

This is a known bug where the cloned repo path isn't passed to the indexer. Fix is in PR #1. Workaround: manually set `root_path` in the projects table to the cloned path.

## Embedding dimension mismatch

All embedding providers must use the same dimension. Gemini and nomic-embed-text both use 768 dimensions. If mixing providers, ensure they match or recreate the Qdrant collection.

## Ollama connection refused

When running Fold in Docker, use the Docker network gateway IP (not localhost):
```bash
docker network inspect fold_default | jq '.[0].IPAM.Config[0].Gateway'
```

## GitHub authentication "too many redirects"

For private repos, use a GitHub PAT (Personal Access Token) with `repo` scope, not an OAuth token.

# Operational Checklist

- [ ] Environment variables configured (.env)
- [ ] Docker network accessible for Ollama (if using local embeddings)
- [ ] Qdrant running and healthy
- [ ] Embedding providers configured (check dimension consistency)
- [ ] LLM provider configured with valid API key
- [ ] API token created for CLI/MCP access
- [ ] UI built with correct VITE_API_URL
- [ ] Traefik routing configured (SSO for UI, token auth for API)
- [ ] Repositories connected with valid access tokens
- [ ] Backups scheduled
