# Fold

**Holographic memory system for development teams.**

A semantic knowledge management system that stores, searches, and retrieves project context across codebases, sessions, decisions, and team activity. Built in Rust with Axum web framework, SQLite for metadata, and Qdrant for vector search.

## Features

- **Holographic retrieval** — Any fragment of knowledge can reconstruct full context
- **Multi-project support** — Isolated memory spaces per project
- **Git integration** — Auto-index GitHub/GitLab repos with webhooks
- **Semantic search** — Find relevant context using natural language queries
- **Knowledge graph** — Explicit links between memories (commits→files, decisions→specs, etc.)
- **Session tracking** — Store coding session notes with automatic context linking
- **Team awareness** — See who's working on what, track decisions and specs
- **LLM fallback chain** — Gemini (free) → OpenRouter → OpenAI
- **MCP compatible** — Works as knowledge layer for Claude, Cursor, and other AI agents
- **Local embeddings** — Generate embeddings locally without external APIs

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Axum Server                             │
├──────────────┬──────────────┬──────────────┬───────────────────┤
│   REST API   │  MCP (JSONRPC) │  Webhooks    │   Admin Auth     │
│  (tokens)    │   (tokens)     │ (signatures) │   (JWT)          │
└──────┬───────┴───────┬───────┴───────┬──────┴────────┬─────────┘
       │               │               │               │
       └───────────────┴───────┬───────┴───────────────┘
                               │
                       ┌───────▼───────┐
                       │    Services   │
                       └───────┬───────┘
                               │
    ┌──────────┬───────────────┼───────────────┬──────────┐
    │          │               │               │          │
┌───▼───┐ ┌────▼────┐   ┌──────▼──────┐  ┌────▼────┐ ┌───▼───┐
│Qdrant │ │ SQLite  │   │LLM Providers│  │ GitHub  │ │GitLab │
│vectors│ │metadata │   │(fallback)   │  │  API    │ │ API   │
└───────┘ └─────────┘   └─────────────┘  └─────────┘ └───────┘
```

## Quick Start

### Prerequisites

- Rust 1.75+
- SQLite 3
- Qdrant running locally or remotely
- At least one LLM API key (Gemini free tier recommended)

### Local Development

```bash
# Clone the repo
git clone https://github.com/Generation-One/fold.git
cd fold/rust

# Copy environment file
cp .env.example .env

# Edit .env with your settings
# - Add at least one LLM API key (GOOGLE_API_KEY, OPENROUTER_API_KEY, or OPENAI_API_KEY)
# - Set ADMIN_BOOTSTRAP_TOKEN for first admin
# - Configure OIDC provider if needed (optional)

# Start Qdrant (requires Docker)
docker run -p 6334:6334 qdrant/qdrant

# In another terminal, run Fold
cargo run
```

Server runs at `http://localhost:8765`

### Web UI

A React-based web interface for Fold is available at:

**[https://github.com/Generation-One/fold-ui](https://github.com/Generation-One/fold-ui)**

Features:
- Dashboard with memory stats and recent activity
- MCP Tester for interactive tool testing
- Settings for token management
- Holographic design system

```bash
# Quick start with Docker
cd fold-ui
docker-compose up -d

# Opens at http://localhost:5174
```

### Docker Compose (Recommended)

```bash
# Start both Fold and Qdrant
docker-compose up -d

# View logs
docker-compose logs -f fold
```

## Configuration

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `HOST` | No | Server host (default: 0.0.0.0) |
| `PORT` | No | Server port (default: 8765) |
| `PUBLIC_URL` | No | Public URL for webhooks (e.g., https://fold.example.com) |
| `DATABASE_PATH` | No | SQLite database path (default: ./data/fold.db) |
| `QDRANT_URL` | No | Qdrant URL (default: http://localhost:6334) |
| `SESSION_SECRET` | Yes | 32-byte secret for session cookies |
| `ADMIN_BOOTSTRAP_TOKEN` | Yes | Token to create first admin (remove after) |

### LLM Providers (pick at least one)

```bash
# Gemini (free tier, priority 1 - recommended)
GOOGLE_API_KEY=your-key
GEMINI_MODEL=gemini-1.5-flash

# OpenRouter (free models available, priority 2)
OPENROUTER_API_KEY=your-key
OPENROUTER_MODEL=meta-llama/llama-3-8b-instruct:free

# OpenAI (priority 3)
OPENAI_API_KEY=your-key
OPENAI_MODEL=gpt-4o-mini
```

### Auth Providers (OIDC)

```bash
# Pattern: AUTH_PROVIDER_{NAME}_{FIELD}

# Example: Corporate OIDC
AUTH_PROVIDER_CORPORATE_TYPE=oidc
AUTH_PROVIDER_CORPORATE_DISPLAY_NAME=Corporate SSO
AUTH_PROVIDER_CORPORATE_ISSUER=https://auth.company.com
AUTH_PROVIDER_CORPORATE_CLIENT_ID=xxx
AUTH_PROVIDER_CORPORATE_CLIENT_SECRET=xxx

# Example: Google
# AUTH_PROVIDER_GOOGLE_TYPE=oidc
# AUTH_PROVIDER_GOOGLE_DISPLAY_NAME=Google
# AUTH_PROVIDER_GOOGLE_ISSUER=https://accounts.google.com
# AUTH_PROVIDER_GOOGLE_CLIENT_ID=xxx
# AUTH_PROVIDER_GOOGLE_CLIENT_SECRET=xxx

# Example: GitHub
# AUTH_PROVIDER_GITHUB_TYPE=github
# AUTH_PROVIDER_GITHUB_DISPLAY_NAME=GitHub
# AUTH_PROVIDER_GITHUB_CLIENT_ID=xxx
# AUTH_PROVIDER_GITHUB_CLIENT_SECRET=xxx
```

## Usage

### REST API

#### Projects

```bash
# Create project (admin only)
curl -X POST http://localhost:8765/projects \
  -H "Authorization: Bearer {api_token}" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-project",
    "slug": "my-project",
    "description": "My awesome project"
  }'

# List projects
curl http://localhost:8765/projects \
  -H "Authorization: Bearer {api_token}"

# Get project details
curl http://localhost:8765/projects/{id} \
  -H "Authorization: Bearer {api_token}"
```

#### Memories

```bash
# Add a memory
curl -X POST http://localhost:8765/projects/{id}/memories \
  -H "Authorization: Bearer {api_token}" \
  -H "Content-Type: application/json" \
  -d '{
    "type": "session",
    "title": "Implemented JWT auth",
    "content": "Added login/logout endpoints with JWT-based sessions...",
    "author": "frank",
    "tags": ["auth", "backend"]
  }'

# Search memories
curl -X POST http://localhost:8765/projects/{id}/search \
  -H "Authorization: Bearer {api_token}" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "how does authentication work?",
    "limit": 10
  }'

# Get context for a task
curl -X POST http://localhost:8765/projects/{id}/context \
  -H "Authorization: Bearer {api_token}" \
  -H "Content-Type: application/json" \
  -d '{
    "task": "Implement password reset flow"
  }'
```

#### Git Integration

```bash
# Connect a GitHub repo
curl -X POST http://localhost:8765/projects/{id}/repositories \
  -H "Authorization: Bearer {api_token}" \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "github",
    "owner": "myorg",
    "repo": "myproject",
    "branch": "main",
    "access_token": "ghp_xxx"
  }'

# List connected repos
curl http://localhost:8765/projects/{id}/repositories \
  -H "Authorization: Bearer {api_token}"
```

### MCP (for AI agents)

Fold exposes an MCP (Model Context Protocol) endpoint for AI coding assistants like Claude Code, Cursor, Windsurf, etc.

#### Getting an API Token

First, create an API token for MCP access:

```bash
# Bootstrap admin (first time only)
curl -X POST http://localhost:8765/auth/bootstrap \
  -H "Content-Type: application/json" \
  -d '{"token": "YOUR_ADMIN_BOOTSTRAP_TOKEN"}'

# Create an API token
curl -X POST http://localhost:8765/auth/tokens \
  -H "Authorization: Bearer {admin_session}" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "claude-code",
    "project_ids": []
  }'
# Returns: { "token": "fold_abc123..." }
```

#### Claude Code Setup

**Option 1: CLI (recommended)**

```bash
claude mcp add -t http -s user fold http://localhost:8765/mcp \
  --header "Authorization: Bearer fold_YOUR_TOKEN_HERE"
```

**Option 2: Manual config**

Edit `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "fold": {
      "url": "http://localhost:8765/mcp",
      "headers": {
        "Authorization": "Bearer fold_YOUR_TOKEN_HERE"
      }
    }
  }
}
```

Then restart Claude Code.

#### Cursor Setup

Add to Cursor settings (`.cursor/mcp.json` or global config):

```json
{
  "mcpServers": {
    "fold": {
      "url": "http://localhost:8765/mcp",
      "headers": {
        "Authorization": "Bearer fold_YOUR_TOKEN_HERE"
      }
    }
  }
}
```

#### Windsurf / Other MCP Clients

Most MCP clients follow the same pattern:
- **URL**: `http://localhost:8765/mcp`
- **Transport**: HTTP (not stdio)
- **Auth**: Bearer token in `Authorization` header

#### Available MCP Tools

| Tool | Description |
|------|-------------|
| `project_list` | List all projects |
| `project_create` | Create a new project |
| `memory_add` | Add a memory to a project |
| `memory_search` | Semantic search across memories |
| `memory_list` | List memories with filters |
| `context_get` | Get relevant context for a task |
| `codebase_index` | Index a project's codebase |
| `codebase_search` | Search indexed code |
| `team_status` | View/update team activity |
| `file_upload` | Upload and index a single file |
| `files_upload` | Batch upload multiple files |

#### Example MCP Usage

Once connected, your AI assistant can use Fold tools:

```
User: "Search for authentication-related memories"
Assistant: [calls mcp__fold__memory_search with query="authentication"]

User: "What context do I need to implement password reset?"
Assistant: [calls mcp__fold__context_get with task="implement password reset"]

User: "Save this session - we fixed the login bug"
Assistant: [calls mcp__fold__memory_add with type="session", content="..."]
```

## Memory Types

| Type | Description | Source |
|------|-------------|--------|
| `codebase` | Source code files | Auto-indexed from git |
| `session` | Coding session notes | Manual or AI-generated |
| `spec` | Requirements & specifications | Manual |
| `decision` | Architecture decisions | Manual |
| `task` | Work items / todos | Manual |
| `commit` | Commit summaries | Auto-generated by LLM |
| `pr` | Pull request summaries | Auto-generated |
| `general` | General knowledge | Manual |

## Knowledge Graph

Memories are connected through a typed graph:

- `modifies` — Commit modified a file
- `contains` — PR contains commits
- `implements` — Task implements a spec
- `decides` — Decision about architecture
- `supersedes` — Newer decision replaces old
- `references` — Cites or mentions
- `related` — Semantically similar

Query the graph to understand relationships:

```bash
# Get all memories that relate to a specific memory
curl http://localhost:8765/projects/{id}/graph/context/{memory_id} \
  -H "Authorization: Bearer {api_token}"

# Get file history (commits and PRs that touched a file)
curl http://localhost:8765/projects/{id}/graph/history/src/auth.rs \
  -H "Authorization: Bearer {api_token}"
```

## Performance Notes

- **Embeddings**: Uses hash-based deterministic embeddings (development) or real fastembed (production)
- **Vector search**: Qdrant handles 1000+ memories efficiently
- **Database**: SQLite is fine for most projects; can migrate to PostgreSQL if needed
- **Webhooks**: Async processing for git integration

## Why "Fold"?

Dimensional collapse — bringing distant but related knowledge close together. In physics, a "fold" in spacetime lets you step between distant points. Fold does this for project knowledge, making any fragment capable of reconstructing the whole context.

## Research & Attribution

Fold builds on foundational work in associative memory and knowledge systems:

### Reference Implementation

- **[A-MEM: Agentic Memory for LLM Agents](https://github.com/WujiangXu/A-mem-sys)** — Wujiang Xu et al.
  The primary codebase this project initally referenced. Implements dynamic memory organization using Zettelkasten principles with auto-generated metadata and inter-memory linking.
  📄 Paper: [arXiv:2502.12110](https://arxiv.org/abs/2502.12110) (NeurIPS 2025)

### Theoretical Foundations

- **Sparse Distributed Memory** — Pentti Kanerva (1988)
  The OG holographic approach to memory. Content-addressable, similarity-based retrieval. Theoretical foundation for how semantic similarity enables retrieval without exact keyword matches.
  📚 *Sparse Distributed Memory*, MIT Press

- **Holographic Reduced Representations** — Tony Plate (1995)
  Distributed compositional representations where meaning is spread across dimensions. Forms the basis of holographic memory systems.

- **Vector Symbolic Architectures** — Ross Gayler (2003)
  Operations on distributed representations for flexible knowledge manipulation.

- **Zettelkasten Method** — Niklas Luhmann
  The note-taking system that inspired A-MEM's interconnected knowledge networks. Each memory is a "slip" with unique identifiers, explicitly linked to related memories.

### Technical Stack

- **[Axum](https://github.com/tokio-rs/axum)** — Modular web framework for Rust
- **[SQLx](https://github.com/launchbadge/sqlx)** — Compile-time checked SQL queries
- **[Qdrant](https://qdrant.tech/)** — Vector database for semantic search
- **[fastembed](https://github.com/qdrant/fastembed-rs)** — Local sentence embeddings
- **[OpenID Connect](https://openid.net/connect/)** — Pluggable authentication

## License

MIT
