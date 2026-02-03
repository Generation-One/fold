# Fold v2 - Complete System Documentation

## Executive Summary

Fold v2 is a **holographic memory system** for codebases where any memory fragment can reconstruct full context through vector similarity and explicit relationships. This document covers the design decisions, architecture, and implementation details.

---

## Design Philosophy

### The Holographic Principle

In a hologram, any fragment contains enough information to reconstruct the whole image (at lower resolution). Fold applies this principle to codebase knowledge:

- **Every memory** contains semantic embeddings that connect it to related knowledge
- **Vector similarity** provides implicit relationships (semantic proximity)
- **Explicit links** provide structural relationships (depends_on, references, etc.)
- **Context reconstruction** walks the graph to build complete understanding

### Key Design Decisions

1. **Unified Memory Type** - Instead of 8 types (codebase, session, spec, decision, commit, PR, general, manual), we use one `Memory` with a `source` field (`File`, `Agent`, `Git`)

2. **Hash-Based Storage** - Content determines identity via SHA256 hash. The first 16 chars become the memory ID, stored at `fold/a/b/hash.md`

3. **Git-Native** - Memories are markdown files committed to the repository. This enables:
   - Version control of knowledge
   - Collaboration via pull requests
   - Sync across machines via git

4. **A-MEM Agentic Evolution** - LLM-powered memory processing:
   - Automatic content analysis for keywords, context, tags
   - Evolution decisions for linking and metadata updates
   - Neighbour metadata updates when new memories join

5. **Decay-Weighted Search** - Re-enabled ACT-R inspired decay:
   - Recent memories score higher
   - Frequently accessed memories persist
   - Combined with semantic similarity for ranking

---

## Architecture Overview

### Storage Locations

| What | Where | Purpose |
|------|-------|---------|
| **Content** | `fold/a/b/hash.md` | Markdown with YAML frontmatter |
| **Metadata** | SQLite `memories` table | id, title, keywords, tags, context |
| **Vectors** | Qdrant collection | Semantic search embeddings |
| **Links** | SQLite `memory_links` table | Related/DependsOn/References |
| **Jobs** | SQLite `jobs` table | Background task queue |

### Database Schema

```sql
-- Core tables (9 total)
projects          -- Project metadata (slug, name, root_path)
repositories      -- Git repo connections (GitHub/GitLab/Local)
memories          -- Memory metadata (content in fold/)
memory_links      -- Relationships between memories

-- Background processing
jobs              -- Background job queue with retry

-- Authentication (OIDC)
users             -- OIDC users
sessions          -- User sessions
api_tokens        -- API authentication
oauth_states      -- OIDC flow state
auth_providers    -- Dynamic OIDC config
```

### Memory Model

```rust
pub struct Memory {
    pub id: String,              // Content hash (16 chars)
    pub project_id: String,
    pub repository_id: Option<String>,

    // Content reference
    pub content_hash: Option<String>,
    pub content_storage: Option<String>,  // "fold"

    // Classification
    pub memory_type: String,     // codebase, session, spec, decision, etc.
    pub source: Option<String>,  // file, agent, git

    // Metadata
    pub title: Option<String>,
    pub author: Option<String>,
    pub keywords: Option<String>,  // JSON array
    pub tags: Option<String>,      // JSON array
    pub context: Option<String>,   // Detailed context (3-5 sentences)

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

### Link Types

```rust
pub enum LinkType {
    Related,     // Semantically related (auto-generated)
    References,  // Explicit reference in content
    DependsOn,   // Dependency relationship
    Modifies,    // Summarizes changes to another memory
}
```

---

## Indexing Pipeline

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           FILE SOURCES                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│  GitHub Webhook  │  GitLab Webhook  │  Polling Loop  │  Manual Trigger      │
│       (push)     │      (push)      │   (5 min)      │   (API/MCP)          │
└────────┬─────────┴────────┬─────────┴───────┬────────┴──────────┬───────────┘
         └──────────────────┴─────────────────┴───────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         JOB QUEUE (SQLite)                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│  • Atomic job claiming (prevents duplicate processing)                      │
│  • Priority-based scheduling                                                 │
│  • Automatic retry with exponential backoff                                 │
│  • Stale job recovery                                                        │
│  • Heartbeat to prevent timeouts                                            │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                      INDEXER SERVICE (indexer.rs)                            │
├─────────────────────────────────────────────────────────────────────────────┤
│  For each file:                                                              │
│  1. Read content from local clone                                            │
│  2. Skip if: empty, >100KB, non-code, excluded pattern                      │
│  3. Calculate SHA256 hash → memory_id (first 16 chars)                      │
│  4. Check cache: skip if hash unchanged                                      │
│  5. Check fold/: skip if fold/a/b/hash.md exists                            │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         LLM SUMMARISATION                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  llm.summarize_code(content, file_path, language)                           │
│                                                                              │
│  Extracts:                                                                   │
│  • title        - Human-readable title (max 100 chars)                       │
│  • summary      - Comprehensive 2-4 sentence description                     │
│  • keywords     - Function names, class names, key terms (max 15)           │
│  • tags         - Categories (auth, database, api, etc.) (max 6)            │
│  • exports      - Public functions, classes, types                          │
│  • dependencies - Imported modules/packages                                  │
│  • created_date - Earliest date found in file (from comments, headers)      │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                     MEMORY SERVICE (memory.rs)                               │
├─────────────────────────────────────────────────────────────────────────────┤
│  1. Auto-analyse content (if auto_metadata=true)                            │
│     → Extract keywords, context (3-5 sentences), tags via LLM              │
│     → Context covers: purpose, role in system, responsibilities,           │
│       architectural patterns, relationships, design decisions               │
│                                                                              │
│  2. Create Memory object with all metadata                                  │
│                                                                              │
│  3. Write to fold/ directory                                                 │
│     → fold/a/b/hash.md with YAML frontmatter                               │
│     → Includes original_date if extracted from file                         │
│                                                                              │
│  4. Insert metadata into SQLite                                              │
│                                                                              │
│  5. Generate embedding                                                       │
│     → embeddings.embed_single(content + context + keywords + tags)          │
│                                                                              │
│  6. Store in Qdrant vector database                                          │
│                                                                              │
│  7. Process memory evolution (A-MEM)                                         │
│     → Find 5 nearest neighbours in Qdrant                                    │
│     → Ask LLM: should we link/evolve?                                       │
│     → Create links in memory_links table                                     │
│     → Update neighbour metadata if needed                                    │
│     → Update fold file with [[wiki-style]] links                            │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         AUTO-COMMIT (git.rs)                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│  If project.auto_commit_enabled() and files were indexed:                   │
│  → git add fold/                                                             │
│  → git commit -m "fold: Index N files from project"                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Memory File Format

### Example File: `fold/a/B/aBcD123456789abc.md`

```markdown
---
id: aBcD123456789abc
title: Authentication Service
author: system
tags:
  - auth
  - typescript
  - security
file_path: src/auth/service.ts
language: typescript
memory_type: codebase
original_date: "2024-03-15"
created_at: 2026-02-03T10:30:00Z
updated_at: 2026-02-03T10:30:00Z
related_to:
  - f0123456789abcde
  - 9a8b7c6d5e4f3g2h
---

This module implements JWT-based authentication with refresh tokens. It serves as the
core authentication layer for the application, handling both user login flows and API
token validation. The service uses RS256 asymmetric signing for enhanced security and
implements refresh token rotation to prevent token theft. Key responsibilities include
token generation, validation with expiry checking, and secure session management.

## Key Components
- AuthService - Main authentication service class
- validateToken() - Token validation with expiry check
- refreshToken() - Refresh token rotation

## Dependencies
- jsonwebtoken, bcrypt

---

## Related

- [[f/0/f0123456789abcde.md|f0123456789abcde]]
- [[9/a/9a8b7c6d5e4f3g2h.md|9a8b7c6d5e4f3g2h]]
```

### Frontmatter Fields

| Field | Description |
|-------|-------------|
| `id` | Content hash (first 16 chars of SHA256) |
| `title` | Human-readable title |
| `author` | Who created this memory (user or "system") |
| `tags` | Category tags for classification |
| `file_path` | Original source file path (if from code) |
| `language` | Programming language |
| `memory_type` | Type: codebase, session, spec, decision, etc. |
| `original_date` | Creation date extracted from file content (if found) |
| `created_at` | When this memory was indexed |
| `updated_at` | Last modification time |
| `related_to` | IDs of linked memories |

### Wiki-Style Links

Related memories are linked using wiki-style markdown links:
```
[[a/b/hash.md|hash]]
```

This format works with Obsidian, Foam, and other markdown editors that support wiki links.

---

## Search and Retrieval

### Decay-Weighted Search

Search combines semantic similarity with memory strength (recency + access frequency):

```
Query
  │
  ▼
Generate embedding
  │
  ▼
Qdrant vector search (top N)
  │
  ▼
For each result:
  │
  ├─ Get memory from SQLite
  │
  ├─ Calculate decay strength:
  │   strength = recency_factor × access_boost
  │   - recency_factor: exponential decay over time (configurable half-life)
  │   - access_boost: log(retrieval_count + 1)
  │
  ├─ Blend scores:
  │   combined = (1 - weight) × semantic + weight × strength
  │
  └─ Update last_accessed, retrieval_count
  │
  ▼
Sort by combined score
  │
  ▼
Return results with decay info
```

### ACT-R Decay Algorithm

Based on cognitive science research, memory strength follows:

```rust
fn calculate_strength(
    updated_at: DateTime<Utc>,
    last_accessed: Option<DateTime<Utc>>,
    retrieval_count: i32,
    half_life_days: f64,
) -> f64 {
    let reference_time = last_accessed.unwrap_or(updated_at);
    let age_days = (Utc::now() - reference_time).num_seconds() as f64 / 86400.0;

    // Exponential decay
    let recency = (-age_days.ln() / half_life_days).exp();

    // Access boost (logarithmic)
    let access_boost = (retrieval_count as f64 + 1.0).ln() / 10.0_f64.ln();

    (recency * (1.0 + access_boost)).clamp(0.0, 1.0)
}
```

### Context Reconstruction

Get holographic context around any memory:

```rust
async fn get_context(&self, memory_id: &str, depth: usize) -> Result<Context> {
    let memory = self.get_memory(memory_id).await?;
    let links = self.db.get_memory_links(memory_id).await?;

    let mut context = Context::new(memory);

    // Add explicitly linked memories
    for link in links {
        let related = self.get_memory(&link.target_id).await?;
        context.add_related(related, link);

        // Recurse for deeper context
        if depth > 1 {
            let sub = self.get_context(&link.target_id, depth - 1).await?;
            context.merge(sub);
        }
    }

    // Add vector-similar memories not explicitly linked
    let similar = self.search(&memory.title, 5).await?;
    for result in similar {
        if !context.has_memory(&result.id) {
            context.add_similar(result);
        }
    }

    Ok(context)
}
```

---

## A-MEM Agentic Memory Evolution

### Content Analysis

When a memory is created with `auto_metadata=true`:

```rust
async fn analyse_content(&self, content: &str) -> Result<ContentAnalysis> {
    let prompt = r#"Analyse this content and extract:

    1. **Keywords**: Key terms and concepts (max 15)
    2. **Context**: A detailed 3-5 sentence summary covering:
       - What this content does and its primary purpose
       - Its role in the broader system or domain
       - Key responsibilities, patterns, or architectural approach
       - Important relationships to other components
       - Notable design decisions or constraints
    3. **Tags**: Broad categories for classification (max 6)
    "#;

    // LLM extracts structured metadata
    self.llm.complete(&prompt, 800).await
}
```

### Memory Evolution

After creating a memory, the system asks the LLM if it should evolve:

```rust
async fn process_memory_evolution(
    &self,
    memory_id: &str,
    embedding: &[f32],
    content: &str,
) -> Result<()> {
    // Find nearest neighbours
    let neighbours = self.qdrant.search(embedding, 5).await?;

    // Build context for LLM
    let neighbour_text = format_neighbours(&neighbours);

    // Ask LLM for evolution decision
    let decision = self.get_evolution_decision(
        &memory,
        content,
        &neighbour_text,
    ).await?;

    if decision.should_evolve {
        // Create links
        for target_id in decision.suggested_connections {
            self.create_link(memory_id, target_id, LinkType::Related).await?;
        }

        // Update neighbour metadata
        for (i, neighbour_id) in neighbours.iter().enumerate() {
            if let Some(new_ctx) = decision.new_context_neighbourhood.get(i) {
                self.update_context(neighbour_id, new_ctx).await?;
            }
        }

        // Update fold file with wiki links
        self.fold_storage.update_memory_links(memory_id, &links).await?;
    }

    Ok(())
}
```

---

## API Endpoints

### Core Endpoints

```
# Projects
POST   /api/projects                    Create project
GET    /api/projects                    List projects
GET    /api/projects/:slug              Get project

# Memories
POST   /api/projects/:slug/memories     Create memory
GET    /api/projects/:slug/memories     List memories
GET    /api/projects/:slug/memories/:id Get memory
PUT    /api/projects/:slug/memories/:id Update memory
DELETE /api/projects/:slug/memories/:id Delete memory

# Search
POST   /api/projects/:slug/search       Vector search with decay
POST   /api/projects/:slug/context/:id  Holographic context

# Links
POST   /api/projects/:slug/memories/:id/links   Create link
GET    /api/projects/:slug/memories/:id/links   List links
DELETE /api/projects/:slug/memories/:id/links/:link_id

# Repositories
POST   /api/projects/:slug/repositories         Connect repo
GET    /api/projects/:slug/repositories         List repos
DELETE /api/projects/:slug/repositories/:id     Disconnect
POST   /api/projects/:slug/repositories/:id/sync  Trigger sync

# Auth (OIDC)
GET    /auth/providers          List configured providers
GET    /auth/login/:provider    Initiate login
GET    /auth/callback/:provider OIDC callback
POST   /auth/logout             Logout
GET    /auth/me                 Current user

# Tokens
GET    /me/tokens               List API tokens
POST   /me/tokens               Create token
DELETE /me/tokens/:id           Revoke token

# Webhooks
POST   /webhooks/github/:repo_id   GitHub webhook
POST   /webhooks/gitlab/:repo_id   GitLab webhook

# Status
GET    /health                  Health check
GET    /status/jobs             Job queue status
GET    /status/jobs/:id         Job details

# MCP
POST   /mcp                     MCP JSON-RPC
GET    /mcp/sse                 MCP Server-Sent Events
```

---

## MCP Tools

```
# Projects
project_list()                          List all projects
project_get(slug)                       Get project details

# Memories
memory_add(project, content, ...)       Create memory
memory_list(project, source?, limit?)   List memories
memory_search(project, query, limit?)   Search with decay
memory_get(project, memory_id)          Get memory
memory_context(project, id, depth?)     Holographic context

# Links
memory_link_add(project, src, tgt, type, ctx?)  Create link
memory_link_list(project, memory_id)            List links

# Indexing
codebase_index(project)                 Trigger full reindex
codebase_search(project, query, limit?) Search code

# Files
files_upload(project, files[], author?) Upload files
```

---

## Features Summary

### Implemented Features

| Feature | Description | File |
|---------|-------------|------|
| **Hash-based storage** | Content-addressed fold/ files | `fold_storage.rs` |
| **LLM summarisation** | Comprehensive code analysis | `llm.rs` |
| **Detailed context** | 3-5 sentence context extraction | `memory.rs` |
| **Date extraction** | Earliest date from file content | `llm.rs`, `indexer.rs` |
| **Wiki-style links** | `[[path/hash.md\|hash]]` format | `fold_storage.rs` |
| **A-MEM evolution** | LLM-driven memory linking | `memory.rs` |
| **Decay-weighted search** | ACT-R inspired scoring | `decay.rs`, `memory.rs` |
| **Auto-commit** | Git commit after indexing | `git.rs` |
| **Job queue** | Background processing with retry | `job_worker.rs` |
| **Multi-provider LLM** | Gemini, Anthropic, OpenAI fallback | `llm.rs` |
| **OIDC auth** | Zitadel, Google, GitHub login | `auth.rs` |

### Removed Features

| Feature | Reason |
|---------|--------|
| 8 memory types | Replaced with unified type + source field |
| Attachments | Removed entirely |
| Team status | Removed entirely |
| AI sessions | Removed entirely |
| Metadata repo sync | Replaced with git-native fold/ |
| Workspaces | Removed entirely |

### Re-enabled Features

| Feature | Reason |
|---------|--------|
| ACT-R decay | Memory strength important for relevance ranking |

---

## Key Files

### Services (`srv/src/services/`)

| File | Purpose |
|------|---------|
| `memory.rs` | Agentic memory with A-MEM evolution |
| `indexer.rs` | File indexing pipeline |
| `fold_storage.rs` | Hash-based fold/ storage |
| `job_worker.rs` | Background job processing |
| `decay.rs` | ACT-R decay algorithm |
| `git.rs` | Auto-commit and sync |
| `llm.rs` | Multi-provider LLM with fallback |
| `embeddings.rs` | Vector embedding generation |
| `qdrant.rs` | Vector database operations |

### Database (`srv/src/db/`)

| File | Purpose |
|------|---------|
| `mod.rs` | Pool init, schema loading |
| `memories.rs` | Memory CRUD operations |
| `links.rs` | Memory link operations |
| `projects.rs` | Project management |
| `jobs.rs` | Job queue operations |
| `users.rs` | User management (OIDC) |

### API (`srv/src/api/`)

| File | Purpose |
|------|---------|
| `memories.rs` | Memory endpoints |
| `mcp.rs` | MCP server tools |
| `auth.rs` | OIDC authentication |
| `webhooks.rs` | GitHub/GitLab webhooks |

---

## Configuration

### Environment Variables

```bash
# Database
DATABASE_PATH=data/fold.db

# Vector store
QDRANT_URL=http://localhost:6333

# LLM providers (at least one required)
GOOGLE_API_KEY=...          # Gemini
ANTHROPIC_API_KEY=...       # Claude
OPENAI_API_KEY=...          # OpenAI
OPENROUTER_API_KEY=...      # OpenRouter

# Embedding
EMBEDDING_PROVIDER=gemini   # or openai
EMBEDDING_MODEL=text-embedding-004
EMBEDDING_DIMENSION=768

# Server
HOST=0.0.0.0
PORT=8765
RUST_LOG=fold=debug,tower_http=debug
```

### Project Configuration (`fold/project.toml`)

```toml
[project]
id = "proj_abc123"
slug = "my-app"
name = "My Application"
created_at = "2026-02-03T10:00:00Z"

[indexing]
include = ["**/*.ts", "**/*.js", "**/*.py", "**/*.rs"]
exclude = ["node_modules/**", "dist/**", ".git/**", "fold/**"]

[embedding]
provider = "gemini"
model = "text-embedding-004"
dimension = 768
```

---

## Next Steps

### UI Updates Required

See `ui/ui-changes.md` for detailed UI changes:

1. Remove type badges from memory list, add source badges
2. Update memory form (remove type selector, add source)
3. Show keywords, context, links in memory detail
4. Simplify search filters
5. Remove Team, Sessions, Attachments navigation

### Future Enhancements

1. **Bidirectional sync** - Push fold/ changes to remote
2. **Conflict resolution** - Handle merge conflicts in fold/
3. **Incremental embedding** - Update embeddings on content change
4. **Graph visualisation** - Visual memory network explorer
5. **Memory consolidation** - Merge similar memories over time
