---
name: managing-fold
description: Use this skill when deploying, configuring, or operating a Fold instance. Triggers on keywords like Fold deployment, Fold configuration, docker-compose setup, Qdrant scaling, LLM provider setup, auth configuration, webhook setup, or production operations for Fold.
---

# Managing Fold

Deploy, configure and operate Fold instances for teams and AI agents.

**Latest Documentation**: Always check [github.com/Generation-One/fold/wiki](https://github.com/Generation-One/fold/wiki) for current deployment guides and configuration options.

# Quick Deployment

## Using Pre-built Images (Recommended)

Always use the official GHCR images rather than building locally:

```bash
# Create stack directory
mkdir -p fold/data/{fold,qdrant}
cd fold

# Create docker-compose.yml (see Unified Stack below)
# Create .env with API keys

# Pull and start
docker compose pull
docker compose up -d

# Check health
curl http://localhost:8765/health
```

**Do not build Docker images locally** — use the versioned releases from `ghcr.io/generation-one/fold` and `ghcr.io/generation-one/fold-ui`.

## Unified Stack

All services should be in a single `docker-compose.yml` for unified stack management:

```yaml
services:
  fold:
    image: ghcr.io/generation-one/fold:latest
    environment:
      - DATABASE_PATH=/data/fold.db
      - QDRANT_URL=http://qdrant:6334
      - GOOGLE_API_KEY=${GOOGLE_API_KEY}
      - OPENROUTER_API_KEY=${OPENROUTER_API_KEY}
    volumes:
      - ./data/fold:/data
      # Mount local paths if using local provider (read-only)
      # - /path/to/project:/path/to/project:ro
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.fold.rule=Host(`fold.example.com`)"
      - "traefik.http.routers.fold.priority=1"
      - "traefik.http.services.fold.loadbalancer.server.port=8765"
    depends_on:
      - qdrant
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8765/health"]
      interval: 10s
      timeout: 5s
      retries: 3
    restart: unless-stopped

  fold-ui:
    image: ghcr.io/generation-one/fold-ui:latest
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.fold-ui.rule=Host(`fold.example.com`) && (Path(`/`) || Path(`/index.html`) || Path(`/fold.svg`) || PathPrefix(`/assets`))"
      - "traefik.http.routers.fold-ui.priority=100"
      - "traefik.http.routers.fold-ui.middlewares=authelia@docker"
      - "traefik.http.services.fold-ui.loadbalancer.server.port=80"
    restart: unless-stopped

  qdrant:
    image: qdrant/qdrant:latest
    volumes:
      - ./data/qdrant:/qdrant/storage
    environment:
      - QDRANT__SERVICE__GRPC_PORT=6334
    healthcheck:
      test: ["CMD-SHELL", "timeout 2 bash -c '</dev/tcp/localhost/6333' || exit 1"]
      interval: 10s
      timeout: 5s
      retries: 5
    restart: unless-stopped
```

**Directory structure:**
```
fold/
├── docker-compose.yml
├── .env
└── data/
    ├── fold/      # SQLite DB, attachments, summaries
    └── qdrant/    # Vector storage
```

Data in `./data/` uses bind mounts — survives restarts and is easy to backup.

## Traefik Routing

**Important**: Fold API has NO `/api` prefix. Routes are directly on root (`/projects`, `/health`, `/jobs`, etc.).

The routing strategy:
- **UI router (high priority)**: Captures only static files — `/`, `/index.html`, `/fold.svg`, `/assets/*`
- **Backend router (low priority)**: Catches everything else — all API routes

```yaml
# UI - captures only static files, with SSO
fold-ui:
  rule: "Host(`fold.example.com`) && (Path(`/`) || Path(`/index.html`) || Path(`/fold.svg`) || PathPrefix(`/assets`))"
  priority: 100
  middlewares:
    - authelia@docker  # SSO for UI only

# Backend - catches everything else, token auth only (no SSO)
fold:
  rule: "Host(`fold.example.com`)"
  priority: 1
  # No auth middleware - uses Bearer token auth
```

The UI uses HashRouter (`/#/path`) so all client-side routes work through `/`.

## Environment Configuration

Create `.env` file:

```bash
# Required
GOOGLE_API_KEY=your-key         # For Gemini embeddings
OPENROUTER_API_KEY=your-key     # For LLM

# Optional
SESSION_SECRET=generate-secure-secret
```

**Note**: Provider configuration is stored in the database, not just env vars.

# Local Filesystem Projects

For indexing local directories (not GitHub repos), use the `local` provider:

```bash
# Create project with local provider
curl -X POST http://localhost:8765/projects \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "slug": "my-project",
    "name": "My Project",
    "provider": "local",
    "root_path": "/path/to/project"
  }'
```

**Important**: The `root_path` must be mounted into the container:

```yaml
volumes:
  - /path/to/project:/path/to/project:ro
```

Use `:ro` (read-only) to prevent accidental modifications.

# Provider Configuration

Providers are configured in SQLite tables `embedding_providers` and `llm_providers`.

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
  '{"endpoint":"http://172.19.0.1:11434","model":"nomic-embed-text","search_priority":1}'
);
```

**Docker networking for Ollama**: Use your Docker network gateway IP:
```bash
docker network inspect fold_default | jq '.[0].IPAM.Config[0].Gateway'
```

## LLM Providers

```sql
-- Update to use Claude via OpenRouter
UPDATE llm_providers 
SET config = '{"endpoint":"https://openrouter.ai/api/v1","model":"anthropic/claude-3-5-haiku-latest"}'
WHERE name = 'openrouter';
```

After modifying providers, restart Fold to reload configuration.

# Known Issues

## 1. floor_char_boundary Panics (UTF-8 truncation)

If indexing panics with "byte index N is not a char boundary", the `floor_char_boundary` helper is missing. This affects both `memory.rs` and `llm.rs`. 

The fix adds a helper function to safely truncate strings containing multi-byte UTF-8 characters (like emojis):

```rust
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

Then use `&content[..floor_char_boundary(&content, content.len().min(4000))]` instead of `&content[..content.len().min(4000)]`.

## 2. Qdrant Healthcheck

Qdrant image doesn't have curl. Use bash TCP check:
```yaml
healthcheck:
  test: ["CMD-SHELL", "timeout 2 bash -c '</dev/tcp/localhost/6333' || exit 1"]
```

## 3. Embedding Dimension Mismatch

All embedding providers must use the same dimension (768 for Gemini/nomic-embed-text). If switching providers, you may need to recreate the Qdrant collection.

# API Quick Reference

**No `/api` prefix** — routes are directly on root:

```bash
# Health
curl http://localhost:8765/health

# List projects
curl -H "Authorization: Bearer $TOKEN" http://localhost:8765/projects

# Get project
curl -H "Authorization: Bearer $TOKEN" http://localhost:8765/projects/{id}

# Trigger reindex
curl -X POST -H "Authorization: Bearer $TOKEN" http://localhost:8765/projects/{id}/reindex

# Search
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "search term", "limit": 10}' \
  http://localhost:8765/projects/{id}/search
```

# Backup

```bash
# SQLite database
docker exec fold-fold-1 sqlite3 /data/fold.db ".backup /data/backup.db"
docker cp fold-fold-1:/data/backup.db ./fold-backup-$(date +%Y%m%d).db

# Qdrant snapshots
curl -X POST http://localhost:6333/collections/fold_memories/snapshots
```

# Operational Checklist

- [ ] Using GHCR images (not local builds)
- [ ] All services in unified docker-compose.yml
- [ ] Data volumes in ./data/ (bind mounts)
- [ ] Environment variables in .env
- [ ] Embedding providers configured (check dimension = 768)
- [ ] LLM provider configured with valid API key
- [ ] Traefik: UI captures static paths only, backend catches rest
- [ ] Local paths mounted if using local provider
- [ ] API token created for CLI/MCP access
