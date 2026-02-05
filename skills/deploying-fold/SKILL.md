---
name: deploying-fold
description: Deploy Fold semantic memory system with Docker. Use when setting up Fold for team/project memory, configuring LLM providers, setting up the UI, or troubleshooting Fold deployments. Triggers on keywords like Fold deployment, Fold setup, semantic memory installation, Qdrant setup, Fold UI, Fold MCP configuration.
---

# Deploying Fold

Fold is a semantic memory system for coding projects. This skill covers deploying both the backend API and optional UI.

## Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Fold UI       │────▶│   Fold API      │────▶│    Qdrant       │
│   (port 80)     │     │   (port 8765)   │     │  (port 6334)    │
└─────────────────┘     └─────────────────┘     └─────────────────┘
        │                       │
        │              ┌────────┴────────┐
        │              │   SQLite DB     │
        │              │   (fold.db)     │
        │              └─────────────────┘
        │
   VITE_API_URL (runtime env var)
```

## Known Build Issues

Before deploying, be aware of these issues that may require manual fixes:

### Rust 1.85+ Build Error (floor_char_boundary)

If building from source fails with `floor_char_boundary is unstable`, the fix is merged in PR #2. Either:
- Pull the latest code: `git pull origin main`
- Or cherry-pick: `git cherry-pick fix/floor-char-boundary`

### Dockerfile Workspace Build

The Dockerfile has been simplified to handle the workspace correctly (PR #3). If you have build failures about missing crates, pull the latest Dockerfile.

## Quick Start

### Option A: Pull from Docker Hub (Recommended)

```bash
# Pull the official image
docker pull generationone/fold:latest

# Or from GitHub Container Registry
docker pull ghcr.io/generation-one/fold:latest
```

Create a `docker-compose.yml`:
```yaml
services:
  fold:
    image: generationone/fold:latest
    ports:
      - "8765:8765"
    environment:
      - GOOGLE_API_KEY=${GOOGLE_API_KEY}
      - QDRANT_URL=http://qdrant:6334
      - PUBLIC_URL=${PUBLIC_URL:-http://localhost:8765}
      - SESSION_SECRET=${SESSION_SECRET}
      - ADMIN_BOOTSTRAP_TOKEN=${ADMIN_BOOTSTRAP_TOKEN}
    volumes:
      - fold-data:/data
    depends_on:
      qdrant:
        condition: service_healthy

  qdrant:
    image: qdrant/qdrant:latest
    ports:
      - "6333:6333"
      - "6334:6334"
    volumes:
      - qdrant-data:/qdrant/storage
    healthcheck:
      test: ["CMD-SHELL", "timeout 2 bash -c '</dev/tcp/localhost/6333' || exit 1"]
      interval: 10s
      timeout: 5s
      retries: 5

volumes:
  fold-data:
  qdrant-data:
```

Create a `.env` file:
```bash
# Required: At least one embedding provider
GOOGLE_API_KEY=your-google-api-key

# Optional but recommended
PUBLIC_URL=https://your-domain.com
SESSION_SECRET=$(openssl rand -hex 32)
ADMIN_BOOTSTRAP_TOKEN=$(openssl rand -hex 32)  # Save this!

# Optional: LLM for AI features
OPENROUTER_API_KEY=sk-or-v1-xxx
ANTHROPIC_API_KEY=sk-ant-xxx
```

Start:
```bash
docker compose up -d
```

### Option B: Build from Source

```bash
git clone https://github.com/Generation-One/fold.git
cd fold

# Create .env from example
cp .env.example .env
# Edit .env with your settings

# Build and start
docker compose up -d --build
```

## Available Docker Tags

| Tag | Description |
|-----|-------------|
| `latest` | Latest stable release from main branch |
| `0.0.1` | Specific version |
| `0.0` | Latest patch for minor version |
| `0` | Latest minor for major version |
| `<sha>` | Specific commit |

## Environment Variables Reference

All configuration is done via environment variables:

### Required (at least one embedding provider)
| Variable | Description |
|----------|-------------|
| `GOOGLE_API_KEY` | Gemini API key for embeddings (recommended) |
| `OPENAI_API_KEY` | OpenAI API key for embeddings |
| `OLLAMA_URL` | Ollama base URL for local embeddings |

### Server
| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `8765` | Listen port |
| `PUBLIC_URL` | `http://localhost:8765` | Public URL for OAuth callbacks |
| `TLS_CERT_PATH` | - | Path to TLS certificate |
| `TLS_KEY_PATH` | - | Path to TLS private key |

### Database
| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_PATH` | `/data/fold.db` | SQLite database path |
| `QDRANT_URL` | `http://localhost:6334` | Qdrant vector DB URL |
| `QDRANT_COLLECTION_PREFIX` | `fold_` | Collection name prefix |

### Auth
| Variable | Default | Description |
|----------|---------|-------------|
| `ADMIN_BOOTSTRAP_TOKEN` | - | Initial admin token for first-time setup |
| `SESSION_SECRET` | auto-generated | Secret for session cookies |
| `SESSION_MAX_AGE` | `604800` | Session lifetime in seconds (7 days) |

OAuth providers use pattern `AUTH_PROVIDER_{NAME}_{FIELD}`:
```bash
AUTH_PROVIDER_GITHUB_TYPE=github
AUTH_PROVIDER_GITHUB_CLIENT_ID=xxx
AUTH_PROVIDER_GITHUB_CLIENT_SECRET=xxx
```

### LLM (optional, for AI features)
| Variable | Default | Description |
|----------|---------|-------------|
| `ANTHROPIC_API_KEY` | - | Claude API key |
| `ANTHROPIC_MODEL` | `claude-3-5-haiku-20241022` | Model name |
| `OPENROUTER_API_KEY` | - | OpenRouter API key |
| `OPENROUTER_MODEL` | `meta-llama/llama-3-8b-instruct:free` | Model name |
| `OPENAI_API_KEY` | - | OpenAI API key |
| `OPENAI_MODEL` | `gpt-4o-mini` | Model name |

### Embeddings
| Variable | Default | Description |
|----------|---------|-------------|
| `GEMINI_EMBEDDING_MODEL` | `text-embedding-004` | Gemini model |
| `OPENAI_EMBEDDING_MODEL` | `text-embedding-3-small` | OpenAI model |
| `OLLAMA_EMBEDDING_MODEL` | `nomic-embed-text:latest` | Ollama model |
| `EMBEDDING_DIMENSION` | `768` | Vector dimension |

### Storage
| Variable | Default | Description |
|----------|---------|-------------|
| `ATTACHMENTS_PATH` | `/data/attachments` | Path for file attachments |
| `SUMMARIES_PATH` | `/data/summaries` | Path for summaries |
| `MAX_ATTACHMENT_SIZE` | `10485760` | Max upload size (10MB) |
| `FOLD_PATH` | `fold` | Base path for memory content |

### Other
| Variable | Default | Description |
|----------|---------|-------------|
| `INDEXING_CONCURRENCY` | `4` | Parallel file indexing limit |
| `RUST_LOG` | `fold=info,tower_http=info` | Log level |

Verify:
```bash
curl http://localhost:8765/health
# {"status":"healthy","version":"0.1.0",...}
```

### 3. Bootstrap Admin User

```bash
curl -X POST http://localhost:8765/auth/bootstrap \
  -H "Content-Type: application/json" \
  -d '{
    "bootstrap_token": "YOUR_ADMIN_BOOTSTRAP_TOKEN",
    "email": "admin@example.com",
    "name": "Admin"
  }'
```

Save the returned API token - this is your admin access.

### 4. Deploy UI (Optional)

```bash
git clone https://github.com/Generation-One/fold-ui.git
cd fold-ui

# Build and run with runtime API URL
docker compose up -d --build
```

Set the API URL at runtime via environment variable:

```yaml
# docker-compose.yml
services:
  fold-ui:
    build: .
    ports:
      - "80:80"
    environment:
      - VITE_API_URL=https://your-domain.com
```

The `VITE_API_URL` is now configured at container startup (not build time), so you can change it without rebuilding.

## Reverse Proxy Setup (Traefik)

Create a dynamic config file:

```yaml
# /path/to/traefik/dynamic/fold.yml
http:
  routers:
    fold-api:
      rule: "Host(`fold.example.com`) && (PathPrefix(`/api`) || PathPrefix(`/health`) || PathPrefix(`/mcp`) || PathPrefix(`/auth`) || PathPrefix(`/projects`) || PathPrefix(`/providers`) || PathPrefix(`/webhooks`))"
      entryPoints:
        - websecure
      tls:
        certResolver: myresolver
      service: fold-api-svc
      priority: 200

    fold-ui:
      rule: "Host(`fold.example.com`)"
      entryPoints:
        - websecure
      tls:
        certResolver: myresolver
      service: fold-ui-svc
      priority: 100

  services:
    fold-api-svc:
      loadBalancer:
        servers:
          - url: "http://172.17.0.1:8765"

    fold-ui-svc:
      loadBalancer:
        servers:
          - url: "http://172.17.0.1:5174"
```

## LLM Provider Configuration

Providers are stored in SQLite. Configure via UI or direct database insert:

```bash
# Get DB path
docker volume inspect fold_fold-data | jq -r '.[0].Mountpoint'

# Add OpenRouter provider
sqlite3 /path/to/fold.db "INSERT INTO llm_providers 
  (id, name, enabled, priority, auth_type, api_key, config) 
  VALUES (
    '$(uuidgen)',
    'openrouter',
    1,
    0,  -- Lower = higher priority
    'api_key',
    'sk-or-v1-xxx',
    '{\"endpoint\":\"https://openrouter.ai/api/v1\",\"model\":\"anthropic/claude-3-5-sonnet\"}'
  )"

# Restart to pick up changes
docker compose restart fold
```

Supported LLM providers: `gemini`, `openai`, `anthropic`, `openrouter`, `claudecode`

### Local Embeddings with Ollama

For air-gapped or cost-sensitive deployments, use Ollama for embeddings:

```bash
# Install embedding model
docker exec ollama ollama pull nomic-embed-text

# Find Docker network gateway (Fold container needs to reach Ollama)
docker network inspect fold_default | jq -r '.[0].IPAM.Config[0].Gateway'
# Returns something like 172.19.0.1

# Add Ollama as embedding provider
sudo sqlite3 /var/lib/docker/volumes/fold_fold-data/_data/fold.db "
INSERT INTO embedding_providers (id, name, enabled, priority, auth_type, config)
VALUES (
  '$(uuidgen)',
  'ollama',
  1,
  100,  -- High priority for indexing
  'none',
  '{\"endpoint\":\"http://172.19.0.1:11434\",\"model\":\"nomic-embed-text\",\"search_priority\":1}'
);"

# Restart to pick up changes
docker compose restart fold
```

**Note:** `priority` controls indexing preference (higher = preferred). `search_priority` in config controls search preference. This lets you use cheap local embeddings for indexing but cloud for search quality.

## MCP Integration

### Generate API Token

Via API:
```bash
curl -X POST https://fold.example.com/api/auth/tokens \
  -H "Authorization: Bearer YOUR_ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "mcp-client"}'
```

### Claude Code Setup

```bash
claude mcp add -t http -s user fold https://fold.example.com/mcp \
  --header "Authorization: Bearer YOUR_API_TOKEN"
```

### Claude Desktop / Cursor / Windsurf

```json
{
  "mcpServers": {
    "fold": {
      "url": "https://fold.example.com/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_API_TOKEN"
      }
    }
  }
}
```

## Common Issues

### UI shows "mixed content" errors
The UI is making HTTP requests from an HTTPS page. Ensure:
1. `VITE_API_URL` uses `https://`
2. Container was restarted after changing the env var

### MCP requests fail with 401
Check that:
1. API token is valid and not revoked
2. Authorization header format is `Bearer <token>`
3. Token has appropriate permissions

### LLM provider not working
After adding providers to database:
```bash
docker compose restart fold
docker logs fold-fold-1 2>&1 | grep "LLM service"
# Should show: LLM service initialized from database [providers]=["openrouter", "gemini"]
```

### Database locked errors
SQLite is single-writer. Don't run multiple Fold instances against the same DB file.

## Updating

### From Docker Hub
```bash
docker compose pull
docker compose up -d
```

### From Source
```bash
cd fold
git pull
docker compose build --no-cache
docker compose up -d

cd ../fold-ui
git pull
docker compose build --no-cache
docker compose up -d
```

## Data Locations

| Data | Location |
|------|----------|
| SQLite DB | Docker volume `fold_fold-data` → `/data/fold.db` |
| Attachments | Docker volume `fold_fold-data` → `/data/attachments/` |
| Qdrant vectors | Docker volume `fold_qdrant-data` |
| FastEmbed cache | Docker volume `fold_fastembed-cache` |

## Backup

```bash
# Stop to ensure consistency
docker compose stop fold

# Backup SQLite
VOLUME_PATH=$(docker volume inspect fold_fold-data -f '{{.Mountpoint}}')
cp $VOLUME_PATH/fold.db ./fold-backup-$(date +%Y%m%d).db

# Restart
docker compose start fold
```

Qdrant data can be recreated by re-indexing, but back up if re-indexing is expensive.
