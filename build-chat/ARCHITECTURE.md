# Architecture

Technical documentation for the Fold backend: services, database schema, API structure, and implementation details.

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────┐
│                        API Layer (Axum)                       │
│  • Memory CRUD endpoints                                      │
│  • Search and context APIs                                    │
│  • MCP protocol server                                        │
│  • Authentication (OIDC)                                      │
└───────────────┬────────────────────────────────┬─────────────┘
                │                                │
┌───────────────▼────────────────┐  ┌────────────▼──────────────┐
│    Services Layer              │  │   Storage Layer          │
│  • Memory service              │  │  • fold/ filesystem      │
│  • Indexer service             │  │  • SQLite database       │
│  • Job worker                  │  │  • Qdrant vectors       │
│  • Decay calculation           │  │  • Git operations       │
│  • LLM integration             │  │                          │
│  • Embeddings generation       │  │                          │
└───────────────┬────────────────┘  └────────────┬─────────────┘
                └───────────────────┬────────────┘
                                    │
                                    ▼
                          ┌─────────────────────┐
                          │  Background Jobs    │
                          │  • Index files      │
                          │  • Process webhooks │
                          │  • Generate embeds  │
                          │  • Auto-commit      │
                          └─────────────────────┘
```

## Key Services

### 1. Memory Service (`src/services/memory.rs`)
Handles memory lifecycle: creation, updates, storage, and A-MEM evolution.

**Key functions:**
- `add()` - Create memory with optional auto-analysis
- `update()` - Modify memory metadata or content
- `get()` - Retrieve memory by ID
- `delete()` - Remove memory
- `process_memory_evolution()` - A-MEM agentic linking

### 2. Indexer Service (`src/services/indexer.rs`)
Processes files from repositories and creates memories.

**Pipeline:**
1. Read file from local clone
2. Skip if: empty, >100KB, non-code, excluded by pattern
3. Calculate SHA256 hash of file path → memory_id (first 16 chars)
4. Check cache: skip if hash unchanged
5. Check fold/: skip if already indexed
6. Summarize via LLM
7. Create memory and store

### 3. Fold Storage Service (`src/services/fold_storage.rs`)
Manages hash-based markdown storage in `fold/a/b/hash.md` for **agent memories only**.

**Content storage by source:**
- **Agent memories**: stored in fold/ directory as markdown files
- **File memories**: LLM summaries stored in SQLite (not in fold/)
- **Git memories**: commit summaries stored in SQLite (not in fold/)

**Format:**
```
fold/
├── a/
│   ├── b/
│   │   ├── aBcD123456789abc.md  (agent memory)
│   │   └── aC12def456789abcd.md (agent memory)
```

Each file contains YAML frontmatter + markdown content. The fold/ directory is reserved for human and AI-authored content (sessions, decisions, specs, tasks, general notes).

### 4. Job Worker (`src/services/job_worker.rs`)
Background task processing with atomic job claiming and retry logic.

**Features:**
- Atomic job claiming (prevents duplicate processing)
- Priority-based scheduling
- Automatic retry with exponential backoff
- Stale job recovery
- Heartbeat to prevent timeouts

**Job types:**
- `index_repo` - Index files from push
- `reindex_repo` - Full repository reindex
- `process_webhook` - Handle webhook events

### 5. Decay Service (`src/services/decay.rs`)
Implements ACT-R inspired memory strength decay.

**Strength formula:**
```
strength = recency_factor × access_boost
recency_factor = exp(-age_days × ln(2) / half_life)
access_boost = log(retrieval_count + 1)
```

### 6. LLM Service (`src/services/llm.rs`)
Multi-provider language model integration (Gemini, Anthropic, OpenAI, OpenRouter).

**Functions:**
- `summarize_code()` - Extract title, summary, keywords, tags, exports, dependencies
- `analyse_content()` - Extract context (3-5 sentences) and metadata
- `suggest_links()` - Propose memory relationships based on semantic analysis
- `get_evolution_decision()` - Decide if new memory should link to neighbours

**Link Creation Flow:**
```
New Memory Created
       │
       ▼
Find 5 nearest neighbours (Qdrant)
       │
       ▼
LLM: "Given this new memory and these neighbours,
      which connections make sense?"
       │
       ▼
LLM returns: {
  should_evolve: true,
  suggested_connections: ["id1", "id2"],
  new_context_neighbourhood: ["updated context for id1"]
}
       │
       ▼
Create memory_links + update neighbour metadata
```

### 7. Embeddings Service (`src/services/embeddings.rs`)
Vector embedding generation via Gemini or OpenAI.

**Config:**
- Provider: `EMBEDDING_PROVIDER` (gemini or openai)
- Model: `EMBEDDING_MODEL` (default: gemini-embedding-001)
- Dimension: `EMBEDDING_DIMENSION` (default: 768)

### 8. Git Service (`src/services/git.rs`)
Manages repository cloning, pulling, and auto-commits.

**Functions:**
- `ensure_clone()` - Clone or pull repository
- `auto_commit_fold()` - Commit fold/ changes
- Webhook integration for GitHub/GitLab

## Database Schema

Nine core tables (stored in SQLite):

```sql
-- Projects and repositories
projects (id, slug, name, root_path, created_at)
repositories (id, project_id, source, url, created_at)

-- Memories and relationships
memories (id, project_id, repository_id, title, type, source,
          keywords, tags, context, file_path, language,
          retrieval_count, last_accessed, created_at, updated_at)
memory_links (id, source_id, target_id, link_type, metadata)

-- Background processing
jobs (id, project_id, job_type, status, priority, payload,
      attempts, next_retry, created_at)

-- Authentication (OIDC)
users (id, provider, email, display_name, avatar_url, role, created_at)
sessions (id, user_id, expires_at, created_at)
api_tokens (id, user_id, token_hash, token_prefix, name, created_at, expires_at, revoked_at)
oauth_states (id, state, provider, expires_at)
auth_providers (id, type, display_name, client_id, enabled)
```

## API Structure

All endpoints under `/api`:

```
# Projects
POST   /projects
GET    /projects
GET    /projects/:slug

# Memories (unified type)
POST   /projects/:slug/memories
GET    /projects/:slug/memories
GET    /projects/:slug/memories/:id
PUT    /projects/:slug/memories/:id
DELETE /projects/:slug/memories/:id

# Search & Context
POST   /projects/:slug/search       (with decay weighting)
POST   /projects/:slug/context/:id  (holographic context)

# Links
POST   /projects/:slug/memories/:id/links
GET    /projects/:slug/memories/:id/links
DELETE /projects/:slug/memories/:id/links/:link_id

# Repositories
POST   /projects/:slug/repositories
GET    /projects/:slug/repositories
DELETE /projects/:slug/repositories/:id
POST   /projects/:slug/repositories/:id/sync

# Auth
GET    /auth/providers
GET    /auth/login/:provider
GET    /auth/callback/:provider
POST   /auth/logout
GET    /auth/me

# Tokens
GET    /me/tokens
POST   /me/tokens
DELETE /me/tokens/:id

# Webhooks
POST   /webhooks/github/:repo_id
POST   /webhooks/gitlab/:repo_id

# Status
GET    /health
GET    /status/jobs
GET    /status/jobs/:id

# MCP
POST   /mcp           (JSON-RPC)
GET    /mcp/sse       (Server-Sent Events)
```

## Authentication & Authorization

Tokens inherit all project access from the user who created them. Access control is handled through project membership:

- **Direct membership** - User assigned to project with role (member/viewer)
- **Group membership** - User in group assigned to project with role
- **Admin bypass** - Admin users have access to all projects
- **Token scope** - Tokens do not have separate project scoping; they inherit user's full access

When a token is used, the system checks:
1. Token is valid (not expired, not revoked)
2. User who owns token has access to requested project
3. User's role (member/viewer) is sufficient for the operation

## MCP Tools

The server exposes tools via Model Context Protocol:

```
# Projects
project_list()
project_get(slug)

# Memories
memory_add(project, content, source, type, ...)
memory_list(project, source?, limit?)
memory_search(project, query, limit?)
memory_get(project, memory_id)
memory_context(project, id, depth?)

# Links
memory_link_add(project, src, tgt, type)
memory_link_list(project, memory_id)

# Indexing
codebase_index(project)
codebase_search(project, query, limit?)

# Files
files_upload(project, files[], author?)
```

## Memory Model

```rust
pub struct Memory {
    pub id: String,                  // Repo path hash (16 chars)
    pub project_id: String,
    pub repository_id: Option<String>,

    // Content storage depends on source:
    // - agent: content in fold/ directory, this field is NULL
    // - file/git: content (LLM summary) stored here in SQLite
    pub content: Option<String>,
    pub content_hash: Option<String>,     // Full SHA256 of content
    pub content_storage: Option<String>,  // DEPRECATED: use source field

    // Classification
    pub memory_type: String,         // codebase, session, spec, decision, etc.
    pub source: Option<String>,      // file, agent, git (determines storage)

    // Metadata
    pub title: Option<String>,
    pub author: Option<String>,
    pub keywords: Option<String>,    // JSON array
    pub tags: Option<String>,        // JSON array
    pub context: Option<String>,     // 3-5 sentence summary

    // For codebase type
    pub file_path: Option<String>,
    pub language: Option<String>,

    // Decay tracking
    pub retrieval_count: i32,
    pub last_accessed: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

## Link Types

```rust
pub enum LinkType {
    Related,     // Semantically related (auto-generated)
    References,  // Explicit reference in content
    DependsOn,   // Dependency relationship
    Modifies,    // Summarizes changes to another memory
}
```

## Development Setup

### Prerequisites

**Windows:**
```powershell
winget install LLVM.LLVM
```

**macOS/Linux:**
- Rust toolchain
- Docker (for Qdrant)

### Build

```bash
cd srv
cargo build          # Development
cargo build --release # Production
```

### Run Locally

```bash
# Start Qdrant
docker run -p 6333:6333 -p 6334:6334 qdrant/qdrant

# Set environment
export RUST_LOG=fold=debug,tower_http=debug
export DATABASE_PATH=data/fold.db
export QDRANT_URL=http://localhost:6333
export GOOGLE_API_KEY=...

# Run server
cargo run
```

Server starts on `0.0.0.0:8765`

### Start/Stop Scripts

```powershell
# Windows PowerShell
cd srv

# Start server (builds and runs)
.\start-server.ps1

# Check status
.\check-server.ps1

# Stop server
.\stop-server.ps1
```

## Environment Variables

### Database & Storage

| Variable | Default | Purpose |
|----------|---------|---------|
| `DATABASE_PATH` | `data/fold.db` | SQLite database location |
| `QDRANT_URL` | `http://localhost:6333` | Vector database URL |

### LLM Providers (at least one required)

| Variable | Purpose |
|----------|---------|
| `GOOGLE_API_KEY` | Gemini API key |
| `ANTHROPIC_API_KEY` | Claude API key |
| `OPENAI_API_KEY` | OpenAI API key |
| `OPENROUTER_API_KEY` | OpenRouter API key |

### Embeddings

| Variable | Default | Purpose |
|----------|---------|---------|
| `EMBEDDING_PROVIDER` | `gemini` | Provider: gemini or openai |
| `EMBEDDING_MODEL` | `gemini-embedding-001` | Model identifier |
| `EMBEDDING_DIMENSION` | `768` | Vector dimension |

### Server

| Variable | Default | Purpose |
|----------|---------|---------|
| `HOST` | `0.0.0.0` | Listen address |
| `PORT` | `8765` | Listen port |
| `RUST_LOG` | `fold=debug` | Log level |

### Decay

| Variable | Default | Purpose |
|----------|---------|---------|
| `DECAY_HALF_LIFE_DAYS` | `30` | Strength halves every N days |
| `DECAY_STRENGTH_WEIGHT` | `0.3` | Blend: 0=semantic, 1=strength |

## Configuration Files

### `.env` (or environment)
```bash
DATABASE_PATH=data/fold.db
QDRANT_URL=http://localhost:6333
GOOGLE_API_KEY=sk-...
RUST_LOG=fold=debug,tower_http=debug
```

### `fold/project.toml` (per project)
```toml
[project]
id = "proj_abc123"
slug = "my-app"
name = "My Application"

[indexing]
include = ["**/*.ts", "**/*.js", "**/*.py", "**/*.rs"]
exclude = ["node_modules/**", "dist/**", ".git/**", "fold/**"]

[embedding]
provider = "gemini"
model = "gemini-embedding-001"
dimension = 768
```

## Key Implementation Details

### Hash-Based Memory IDs

Memory IDs are derived from repo path hash:
```
path_hash = SHA256(project_slug + "/" + normalised_path)[0..16]  // First 16 chars
memory_id = path_hash
storage_path = fold/{first_char}/{second_char}/{hash}.md
```

The path is normalised (forward slashes, relative to repo root) to ensure consistent IDs across machines. This makes memory identity deterministic and stable across content changes.

### Indexing Pipeline

1. **File Sources** - GitHub webhook, GitLab webhook, polling (5 min), manual trigger
2. **Job Queue** - Atomic job claiming prevents duplicates
3. **Indexer** - Skip unchanged/excluded files, summarize via LLM
4. **Memory Service** - Store in fold/, SQLite, Qdrant
5. **A-MEM Evolution** - Find neighbours, ask LLM for linking
6. **Auto-Commit** - Commit fold/ changes if enabled

### A-MEM Agentic Evolution (LLM-Powered Linking)

When a memory is created, the LLM automatically suggests and creates links:

1. **Find neighbours** - Query Qdrant for 5 nearest vector neighbours
2. **Build context** - Format neighbour titles, summaries, and tags for LLM
3. **LLM decision** - Ask LLM: "Should we link these memories? Which connections make sense?"
4. **Create links** - Insert memory_links for each LLM-suggested connection
5. **Update neighbours** - LLM may suggest context updates for related memories
6. **Write wiki links** - Add `[[path/hash.md|hash]]` links to fold file

The LLM considers semantic similarity, functional relationships, and architectural patterns when suggesting links. This creates a knowledge graph that reflects actual code relationships, not just keyword matches.

### Decay-Weighted Search

Search combines semantic similarity with memory strength:
```
combined_score = (1 - weight) × semantic + weight × strength

strength = exp(-age_days × ln(2) / half_life) + log(access_count + 1)
```

Default weight: 0.3 (70% semantic, 30% strength)

## Testing

```bash
# Run tests
cargo test

# With logs
RUST_LOG=fold=debug cargo test -- --nocapture

# Specific test
cargo test service::memory::tests::
```

## Production Deployment

See `/docs/Deployment-Operations.md` for:
- Docker Compose setup
- Reverse proxy configuration
- LLM provider setup
- Monitoring and scaling
- Backup and recovery

## Common Issues

**Server won't start:**
- Check Qdrant is running: `curl http://localhost:6333/health`
- Check DATABASE_PATH is writable
- Check LLM provider keys are set

**Indexing fails:**
- Check local clone exists in `FOLD_CLONES_PATH`
- Check LLM provider connectivity
- Check Qdrant disk space

**Memory not indexed:**
- Check job queue: `GET /status/jobs`
- Check file not in exclude patterns
- Check file size < 100KB

## Useful Commands

```bash
# Check server status
curl http://localhost:8765/health

# List jobs
curl http://localhost:8765/status/jobs

# Check specific job
curl http://localhost:8765/status/jobs/{job_id}

# Trigger reindex
curl -X POST http://localhost:8765/api/projects/my-app/sync
```

## File Organization

```
srv/
├── src/
│   ├── main.rs              # Server entrypoint
│   ├── config.rs            # Configuration
│   ├── api/                 # API handlers
│   │   ├── memories.rs      # Memory endpoints
│   │   ├── auth.rs          # Auth endpoints
│   │   ├── webhooks.rs      # Webhook handlers
│   │   └── mcp.rs           # MCP protocol
│   ├── services/            # Business logic
│   │   ├── memory.rs        # Memory lifecycle
│   │   ├── indexer.rs       # File indexing
│   │   ├── fold_storage.rs  # fold/ filesystem
│   │   ├── job_worker.rs    # Background jobs
│   │   ├── decay.rs         # ACT-R decay
│   │   ├── llm.rs           # LLM integration
│   │   ├── embeddings.rs    # Vector generation
│   │   └── git.rs           # Git operations
│   ├── db/                  # Database layer
│   │   ├── mod.rs           # Pool and schema
│   │   ├── memories.rs      # Memory queries
│   │   ├── links.rs         # Link queries
│   │   ├── projects.rs      # Project queries
│   │   └── jobs.rs          # Job queue
│   └── models/              # Data structures
├── schema.sql               # Database schema
├── start-server.ps1        # Windows start script
├── check-server.ps1        # Windows status script
└── stop-server.ps1         # Windows stop script
```

## Contributing

When modifying core services:

1. **Memory Service** - Handle metadata updates, A-MEM evolution
2. **Indexer Service** - Maintain file processing logic
3. **Fold Storage** - Keep hash-based structure intact
4. **Job Worker** - Ensure atomic job claiming
5. **API Layer** - Update endpoint signatures carefully

All changes should maintain backward compatibility with existing memories in fold/.
