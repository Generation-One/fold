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
│   (port 5174)   │     │   (port 8765)   │     │  (port 6334)    │
└─────────────────┘     └─────────────────┘     └─────────────────┘
        │                       │
        │              ┌────────┴────────┐
        │              │   SQLite DB     │
        │              │   (fold.db)     │
        │              └─────────────────┘
        │
   VITE_API_URL (build-time)
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

### 1. Clone and Configure Backend

```bash
git clone https://github.com/Generation-One/fold.git
cd fold

# Create .env from example
cp .env.example .env
```

Edit `.env`:
```bash
# Required
PUBLIC_URL=https://your-domain.com  # External URL
SESSION_SECRET=$(openssl rand -hex 32)
ADMIN_BOOTSTRAP_TOKEN=$(openssl rand -hex 32)  # Save this!

# LLM Provider (at least one required for summaries)
OPENROUTER_API_KEY=sk-or-v1-xxx
OPENROUTER_MODEL=anthropic/claude-3-5-sonnet

# Or use Gemini (free tier)
GOOGLE_API_KEY=xxx
GEMINI_MODEL=gemini-1.5-flash
```

### 2. Start Backend

```bash
docker compose up -d
```

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

# Set API URL
echo "VITE_API_URL=https://your-domain.com" > .env

# Build and run
docker compose up -d --build
```

**Critical:** `VITE_API_URL` must be set before build - it's baked into the JS bundle.

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
2. UI was rebuilt after setting the env var
3. Docker image was rebuilt with `--no-cache`

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
