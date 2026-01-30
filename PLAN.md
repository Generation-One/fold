# Fold Rust Conversion Plan

## Overview

**Holographic memory system** for development teams - semantic knowledge storage with git integration and multi-provider LLM support.

### Theoretical Foundation: Holographic Memory

Fold implements principles from **holographic/distributed memory** research:

| Principle | Origin | How Fold Implements It |
|-----------|--------|------------------------|
| **Content-addressable** | Holographic Associative Memory (Gabor, Longuet-Higgins) | Semantic search by meaning, not exact match |
| **Distributed representation** | Vector Symbolic Architectures (Kanerva) | Embeddings spread meaning across dimensions |
| **Graceful degradation** | Sparse Distributed Memory | Similar queries → similar results; no cliff |
| **Reconstruction from cues** | HRR (Plate, 1995) | `context_get` reconstructs relevant context from task description |
| **Superposition** | Holographic Reduced Representations | Multiple memories coexist in vector space; similarity = proximity |
| **Associative binding** | Tensor Product Representations | Graph links bind memories together explicitly |

**Key papers respected**:
- Kanerva, P. (1988). *Sparse Distributed Memory* - Content-addressable, graceful degradation
- Plate, T. (1995). *Holographic Reduced Representations* - Distributed compositional representations
- Gayler, R. (2003). *Vector Symbolic Architectures* - Operations on distributed representations
- Graves et al. (2014). *Neural Turing Machines* - External memory for neural systems

**Where Fold extends beyond pure holographic memory**:

1. **Explicit graph structure** - Pure holographic memory uses only similarity; Fold adds typed links (`modifies`, `implements`, `supersedes`) for explicit relationships that can't be captured by embedding proximity alone

2. **Temporal structure** - Sessions, commit history, "what changed" queries require time-aware retrieval beyond static holographic storage

3. **Multi-resolution** - Summaries + full content + links provide multiple levels of detail (holographic memory typically operates at one resolution)

4. **Typed retrieval** - Different memory types (code, decisions, specs) have different retrieval patterns; pure holographic memory is type-agnostic

**The "holographic" property in practice**:
```
Any fragment can reconstruct the whole context:
- File path → related commits → related decisions → related specs → team activity
- Task description → relevant files → recent changes → blocking issues
- Commit SHA → files changed → PR it belongs to → spec it implements
```

The graph + embeddings together create a system where you can "enter" the knowledge from any point and reconstruct relevant context - similar to how a holographic fragment contains the whole image.

---

## Understanding Holographic Memory (Non-Technical)

### What Is It?

Imagine a library where every book somehow knows about every other book. Ask about "authentication" and you don't just get books with that word in the title—you get books about security, user sessions, login flows, and that decision your team made six months ago about using JWT tokens. The library understands *meaning*, not just words.

That's what Fold does for your project's knowledge.

### Why "Holographic"?

In a traditional photograph, if you tear off a corner, that corner is simply gone. But a hologram is different—tear off a piece, and remarkably, you can still see the whole image, just a bit fuzzier. The entire picture is somehow encoded in every fragment.

Fold works the same way with your project knowledge:

- **Start anywhere, reach everywhere** — Begin with a file name, and Fold can trace you to the commits that changed it, the decisions that shaped it, the specs it implements, and who's been working on it recently.

- **Partial recall works** — Can't remember the exact name? Describe what you're looking for in your own words. "That thing we did with the login timeout" will find the relevant code, commits, and decisions.

- **Nothing exists in isolation** — Every piece of knowledge is connected to related pieces. A commit isn't just a commit—it's linked to the files it touched, the task it completed, and the architectural decisions it followed.

### How Does It Actually Work?

When you add something to Fold—whether it's a code file, a design decision, or session notes—three things happen:

1. **Understanding** — An AI reads the content and grasps what it's *about*, not just what words it contains. "Authentication module" and "login system" are understood as related concepts.

2. **Positioning** — The content gets placed in a vast conceptual space where similar things are near each other. Code about user sessions sits close to code about authentication, which sits near decisions about security.

3. **Connecting** — Explicit links are created: this commit modified these files, this decision affects these components, this spec is implemented by these tasks.

When you search, you're not matching keywords—you're asking "what in this space is close to what I'm describing?" The answer might use completely different words but be exactly what you needed.

### The Practical Upshot

**For developers**: Ask "what do I need to know to work on the payment system?" and get back not just code, but recent changes, team decisions, specs, and what your colleagues have been doing in that area.

**For AI assistants**: When Claude or another AI helps with your code, it can draw on your entire project history—understanding not just *what* the code does, but *why* it was written that way and *how* it connects to everything else.

**For teams**: Knowledge doesn't disappear when someone goes on holiday. The reasoning behind decisions, the context around changes, the connections between components—it's all preserved and findable.

### A Quick Example

Say you're debugging a session timeout issue. In a traditional system, you might search for "session" and "timeout" and hope you find something.

With Fold, you describe the problem: "sessions are expiring too early". Fold might return:

- The authentication module (because it handles sessions)
- A commit from last week titled "Reduce session timeout for security" (probably the culprit)
- A decision document stating "sessions should last 7 days" (which contradicts that commit)
- Notes from Jane's coding session where she mentioned security concerns

You've gone from a vague symptom to likely cause and context in seconds, without knowing exactly what to search for.

### The Name "Fold"

Think of it as folding space—bringing distant but related things close together. In physics, a "fold" in spacetime would let you step from one place to another without traversing the distance between them.

Fold does this for knowledge. The authentication code and the security decision made eight months ago are conceptually close, even though they live in entirely different places. Fold lets you step directly between them.

---
- **Holographic Memory**: Store and search project knowledge - any fragment reconstructs full context
- **Git Integration**: Connect GitHub/GitLab repos, auto-index on push via webhooks
- **LLM Fallback Chain**: Gemini (free) → OpenRouter → OpenAI, configurable priority
- **Simple Auth**: Admin users + API tokens scoped to projects
- **MCP Server**: Expose tools for AI agents
- **Management API**: Full CRUD for future UI

### Tech Stack
- **Rust** + Axum (web framework)
- **SQLite** (metadata, users, tokens)
- **Qdrant** (vector storage)
- **fastembed** (local embeddings)

---

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

---

## Auth Model

### OIDC Authentication (Users)

```
┌─────────────────────────────────────────────────────────────┐
│                    OIDC Providers (Pluggable)               │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │ Zitadel  │  │  Google  │  │  GitHub  │  │ Custom   │    │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘    │
│       └─────────────┴─────────────┴─────────────┘          │
│                           │                                 │
│              Uses OIDC Discovery when available            │
│            (/.well-known/openid-configuration)             │
└───────────────────────────┬─────────────────────────────────┘
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                        Fold User                            │
│  - Created on first OIDC login                              │
│  - Linked to provider + subject (sub claim)                 │
│  - Role: admin | member                                     │
│  - First user auto-promoted to admin                        │
└───────────────────────────┬─────────────────────────────────┘
                            │ can create
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                       API Tokens                            │
│  - Bearer token for API/MCP access                          │
│  - Scoped to specific projects [a, b, c]                    │
│  - Used by: MCP consumers, CI, scripts                      │
│  - Managed via UI by authenticated users                    │
└─────────────────────────────────────────────────────────────┘
```

### Provider Configuration

Providers configured via environment variables with a generic pattern:

```bash
# Pattern: AUTH_PROVIDER_{NAME}_{FIELD}
# NAME = arbitrary identifier (uppercase, used in URLs like /auth/login/corporate)

# Example 1: Corporate Zitadel
AUTH_PROVIDER_CORPORATE_TYPE=oidc
AUTH_PROVIDER_CORPORATE_DISPLAY_NAME=Corporate SSO
AUTH_PROVIDER_CORPORATE_ISSUER=https://auth.company.com
AUTH_PROVIDER_CORPORATE_CLIENT_ID=xxx
AUTH_PROVIDER_CORPORATE_CLIENT_SECRET=xxx
AUTH_PROVIDER_CORPORATE_SCOPES=openid profile email  # optional, defaults shown

# Example 2: Google
AUTH_PROVIDER_GOOGLE_TYPE=oidc
AUTH_PROVIDER_GOOGLE_DISPLAY_NAME=Google
AUTH_PROVIDER_GOOGLE_ISSUER=https://accounts.google.com
AUTH_PROVIDER_GOOGLE_CLIENT_ID=xxx
AUTH_PROVIDER_GOOGLE_CLIENT_SECRET=xxx

# Example 3: GitHub (OAuth2, not OIDC)
AUTH_PROVIDER_GITHUB_TYPE=github
AUTH_PROVIDER_GITHUB_DISPLAY_NAME=GitHub
AUTH_PROVIDER_GITHUB_CLIENT_ID=xxx
AUTH_PROVIDER_GITHUB_CLIENT_SECRET=xxx

# Example 4: GitLab (self-hosted)
AUTH_PROVIDER_GITLAB_TYPE=oidc
AUTH_PROVIDER_GITLAB_DISPLAY_NAME=GitLab
AUTH_PROVIDER_GITLAB_ISSUER=https://gitlab.company.com
AUTH_PROVIDER_GITLAB_CLIENT_ID=xxx
AUTH_PROVIDER_GITLAB_CLIENT_SECRET=xxx

# Example 5: Auth0
AUTH_PROVIDER_AUTH0_TYPE=oidc
AUTH_PROVIDER_AUTH0_DISPLAY_NAME=Auth0
AUTH_PROVIDER_AUTH0_ISSUER=https://your-tenant.auth0.com
AUTH_PROVIDER_AUTH0_CLIENT_ID=xxx
AUTH_PROVIDER_AUTH0_CLIENT_SECRET=xxx
```

**Provider types**:
| Type | Discovery | Notes |
|------|-----------|-------|
| `oidc` | `{issuer}/.well-known/openid-configuration` | Standard OIDC (Zitadel, Google, Auth0, Keycloak, Okta, etc.) |
| `github` | Hardcoded endpoints | GitHub OAuth2 + user API (not OIDC) |
| `gitlab` | `{issuer}/.well-known/openid-configuration` | GitLab OIDC (cloud or self-hosted) |

**Optional fields**:
```bash
AUTH_PROVIDER_{NAME}_SCOPES=openid profile email    # Space-separated, defaults shown
AUTH_PROVIDER_{NAME}_ICON=github                    # For UI (github, google, microsoft, key)
AUTH_PROVIDER_{NAME}_ENABLED=true                   # Can disable without removing
```

**API response** (for login UI):
```json
GET /auth/providers

[
  { "id": "corporate", "display_name": "Corporate SSO", "icon": "key" },
  { "id": "google", "display_name": "Google", "icon": "google" },
  { "id": "github", "display_name": "GitHub", "icon": "github" }
]
```

**Login flow**: `/auth/login/{provider_id}` → redirects to provider

### API Token Format
```
fold_xxxxxxxxxxxxxxxxxxxxxxxxxxxx
     └─ 28 random chars (base62)
```

### Admin Bootstrap

First admin must be created with a bootstrap token (not auto-promoted):

```bash
# Set in environment
ADMIN_BOOTSTRAP_TOKEN=your-secret-token

# First admin creation (one-time)
POST /auth/bootstrap
{
  "token": "your-secret-token",
  "provider": "zitadel",
  "subject": "user-sub-from-oidc"  # Or email, depends on provider
}
```

After first admin exists, they can promote other users via UI.

### Session Management

- OIDC login → session cookie (httponly, secure)
- Session stored in SQLite with expiry
- API tokens use Bearer header (no session)

---

## LLM Fallback Chain

Configurable provider priority with automatic fallback on failure/rate-limit.

```rust
providers:
  - name: gemini
    base_url: https://generativelanguage.googleapis.com/v1beta
    model: gemini-1.5-flash
    api_key: ${GOOGLE_API_KEY}
    priority: 1  # Try first (free tier)

  - name: openrouter
    base_url: https://openrouter.ai/api/v1
    model: meta-llama/llama-3-8b-instruct:free
    api_key: ${OPENROUTER_API_KEY}
    priority: 2  # Fallback

  - name: openai
    base_url: https://api.openai.com/v1
    model: gpt-4o-mini
    api_key: ${OPENAI_API_KEY}
    priority: 3  # Last resort
```

On request:
1. Try provider with priority 1
2. If error/rate-limit → try priority 2
3. Continue until success or all fail

---

## Storage Model

### Memory Types & Storage

| Type | Content Storage | Vector Storage | Use Case |
|------|-----------------|----------------|----------|
| `codebase` | Summary only (SQLite) | Summary embedding | Git-indexed source files |
| `session` | Full content (SQLite) | Content embedding | Coding session notes |
| `spec` | Full content (SQLite) | Content embedding | Feature specifications |
| `decision` | Full content (SQLite) | Content embedding | Architectural decisions |
| `task` | Full content (SQLite) | Content embedding | TODOs, work items |
| `general` | Full content (SQLite) | Content embedding | Anything else |
| `commit` | **LLM-generated summary** | Summary embedding | Git commit summaries (auto-generated) |
| `pr` | Title + description | Combined embedding | Pull requests |

**Note on `commit` type**: Raw commit data (sha, message, files) stored in `git_commits` table. The `commit` memory contains an LLM-generated summary of what changed and why - this is what gets embedded and searched.

### File Attachments

Memories can have file attachments (images, PDFs, documents).

```
data/
├── fold.db              # SQLite database
├── attachments/         # File uploads
│   └── {project_slug}/
│       └── {attachment_id}.{ext}
└── summaries/           # Commit summaries as markdown
    └── {project_slug}/
        └── {repo_owner}-{repo_name}/
            └── YY/MM/
                └── DD-HH-MM-{sha}.md
```

```sql
-- Attachments table
attachments (
  id TEXT PRIMARY KEY,
  memory_id TEXT NOT NULL REFERENCES memories(id),
  filename TEXT NOT NULL,        -- Original filename
  content_type TEXT NOT NULL,    -- MIME type
  size_bytes INTEGER NOT NULL,
  storage_path TEXT NOT NULL,    -- Relative path in attachments/
  created_at TEXT NOT NULL
)
```

**Supported types**: images (png, jpg, gif, webp), documents (pdf, md, txt), data (json, csv)

**Size limit**: 10MB per file (configurable)

### Memory Relationships (Graph System)

The relationship system is **core** to Fold - it builds a knowledge graph connecting all memories.

#### Link Types

| Type | Direction | Auto-generated? | Example |
|------|-----------|-----------------|---------|
| `modifies` | commit → codebase | ✅ Yes | Commit summary links to files it changed |
| `contains` | pr → commit | ✅ Yes | PR links to its commits |
| `affects` | pr → codebase | ✅ Yes | PR links to files it touched |
| `implements` | task → spec | Manual/AI | Task that builds a spec |
| `decides` | decision → codebase | Manual/AI | Decision about how code should work |
| `supersedes` | memory → memory | Manual | New decision replaces old one |
| `references` | memory → memory | Manual/AI | Any citation or mention |
| `related` | memory → memory | AI-suggested | Semantically similar |
| `parent` | memory → memory | Manual | Hierarchical (spec → sub-specs) |
| `blocks` | task → task | Manual | Dependency |
| `caused_by` | commit → task | Manual/AI | Commit that closes a task |

#### Auto-Generated Links

When a commit is processed:
```
1. Create commit summary memory
2. For each file changed:
   a. Find/create codebase memory for that file
   b. Create link: commit --modifies--> codebase_file
   c. Store: additions, deletions, change_type (add/modify/delete)
3. If commit message references issue/PR:
   a. Create link: commit --caused_by--> task (if task memory exists)
```

When a PR is processed:
```
1. Create PR memory
2. For each commit in PR:
   a. Create link: pr --contains--> commit
3. For each file touched:
   a. Create link: pr --affects--> codebase_file
```

#### Link Schema

```sql
-- Memory links (edges in the knowledge graph)
memory_links (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),

  source_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  target_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  link_type TEXT NOT NULL,

  -- Metadata
  created_by TEXT NOT NULL,       -- 'system' | 'user' | 'ai'
  confidence REAL,                -- For AI-suggested links (0.0-1.0)
  context TEXT,                   -- Why this link exists

  -- For code links
  change_type TEXT,               -- 'added', 'modified', 'deleted' (for modifies)
  additions INTEGER,              -- Lines added
  deletions INTEGER,              -- Lines deleted

  created_at TEXT NOT NULL,

  UNIQUE(source_id, target_id, link_type)
)

CREATE INDEX idx_links_source ON memory_links(source_id);
CREATE INDEX idx_links_target ON memory_links(target_id);
CREATE INDEX idx_links_type ON memory_links(project_id, link_type);
```

#### Graph Queries

**API**:
```
# Basic link operations
POST   /projects/{id}/memories/{mid}/links          # Add link
GET    /projects/{id}/memories/{mid}/links          # List links (in + out)
DELETE /projects/{id}/memories/{mid}/links/{lid}    # Remove link

# Graph traversal
GET    /projects/{id}/graph/neighbors/{mid}         # Direct connections
GET    /projects/{id}/graph/path?from={a}&to={b}    # Find path between memories
GET    /projects/{id}/graph/subgraph?ids={a,b,c}    # Get subgraph containing memories
GET    /projects/{id}/graph/cluster/{mid}?depth=2   # Get cluster around memory

# Smart queries
GET    /projects/{id}/graph/history/{file_path}     # All commits/PRs that touched file
GET    /projects/{id}/graph/context/{mid}           # Rich context (decisions, specs, related)
GET    /projects/{id}/graph/impact/{mid}            # What would be affected if this changes
```

**Example: File History**
```json
GET /projects/myproj/graph/history/src/auth.rs

{
  "file": { "id": "mem_auth_rs", "path": "src/auth.rs", "type": "codebase" },
  "timeline": [
    {
      "date": "2025-01-15T14:32:00Z",
      "type": "commit",
      "memory": { "id": "mem_commit_abc", "title": "Add JWT validation" },
      "changes": { "additions": 45, "deletions": 12 }
    },
    {
      "date": "2025-01-10T09:15:00Z",
      "type": "pr",
      "memory": { "id": "mem_pr_42", "title": "Implement auth flow" }
    }
  ],
  "decisions": [
    { "id": "mem_dec_jwt", "title": "Use JWT for session tokens" }
  ],
  "specs": [
    { "id": "mem_spec_auth", "title": "Authentication System Spec" }
  ]
}
```

**Example: Commit Context**
```json
GET /projects/myproj/graph/context/mem_commit_abc

{
  "memory": { "id": "mem_commit_abc", "type": "commit", "title": "Add JWT validation" },
  "files_modified": [
    { "id": "mem_auth_rs", "path": "src/auth.rs", "additions": 45, "deletions": 12 },
    { "id": "mem_middleware", "path": "src/middleware/jwt.rs", "additions": 120, "deletions": 0 }
  ],
  "part_of_pr": { "id": "mem_pr_42", "title": "Implement auth flow", "number": 42 },
  "implements_task": { "id": "mem_task_auth", "title": "Add user authentication" },
  "related_decisions": [
    { "id": "mem_dec_jwt", "title": "Use JWT for session tokens", "relevance": 0.92 }
  ],
  "related_specs": [
    { "id": "mem_spec_auth", "title": "Authentication System Spec", "relevance": 0.88 }
  ]
}
```

#### AI-Suggested Links

When a new memory is created, optionally run AI to suggest links:

```
1. Embed the new memory
2. Find top-k similar memories
3. For each candidate:
   a. Ask LLM: "Does memory A relate to memory B? How?"
   b. If yes, suggest link with type and confidence
4. Store suggestions as pending (user can approve/reject)
```

**Suggestion API**:
```
GET  /projects/{id}/memories/{mid}/suggested-links   # Get AI suggestions
POST /projects/{id}/memories/{mid}/suggested-links/{sid}/approve
POST /projects/{id}/memories/{mid}/suggested-links/{sid}/reject
```

#### MCP Tools for Graph

```
memory_links_add        # Add a link between memories
memory_links_list       # List links for a memory
memory_graph_context    # Get rich context around a memory
memory_graph_history    # Get history of a file/component
memory_graph_related    # Find related memories (semantic + graph)
```

#### Graph Visualization (for UI)

The API returns data suitable for graph visualization:

```json
GET /projects/{id}/graph/subgraph?ids=mem_a,mem_b&depth=1

{
  "nodes": [
    { "id": "mem_a", "type": "commit", "title": "Add auth", "x": 0, "y": 0 },
    { "id": "mem_b", "type": "codebase", "title": "auth.rs", "x": 100, "y": 50 },
    { "id": "mem_c", "type": "decision", "title": "Use JWT", "x": -50, "y": 100 }
  ],
  "edges": [
    { "source": "mem_a", "target": "mem_b", "type": "modifies", "label": "+45/-12" },
    { "source": "mem_a", "target": "mem_c", "type": "implements" }
  ],
  "clusters": [
    { "id": "auth", "label": "Authentication", "nodes": ["mem_a", "mem_b", "mem_c"] }
  ]
}
```

Supports:
- Force-directed layout hints (x, y coordinates)
- Clustering by topic/component
- Edge labels with metadata
- Node sizing by importance (connection count)

#### In Markdown Sync

Links are preserved in frontmatter:
```markdown
---
id: mem_commit_abc
type: commit
sha: abc123
links:
  - type: modifies
    target: mem_auth_rs
    additions: 45
    deletions: 12
  - type: modifies
    target: mem_middleware
    additions: 120
    deletions: 0
  - type: caused_by
    target: mem_task_auth
---

# Add JWT validation

This commit implements JWT-based authentication...
```

---

## Git Integration

### Flow: Adding a Repository

```
1. Admin creates project with repo_url + git_token
2. Admin configures which branches to monitor
3. Fold clones/fetches repo metadata via API
4. Fold indexes all files from monitored branches (creates memories)
5. Fold registers webhook with GitHub/GitLab
6. On push → webhook triggers → if branch monitored → index changed files
```

### Single Branch Per Repository

Each repository monitors exactly one branch (keeps it simple):

```json
POST /projects/{id}/repositories
{
  "provider": "github",
  "owner": "myorg",
  "repo": "myproject",
  "access_token": "ghp_xxx",
  "branch": "main"           // Single branch, defaults to repo's default branch
}
```

To monitor multiple branches → connect the same repo multiple times with different branches.

### File Hashing for Safe Upserts

Every indexed file gets a deterministic ID based on its identity:

```rust
// Deterministic memory ID for codebase files
fn file_memory_id(project_id: &str, repo_id: &str, file_path: &str) -> String {
    let input = format!("{}:{}:{}", project_id, repo_id, file_path);
    format!("mem_{}", sha256_hex(&input)[..24])
}

// Content hash to detect changes
fn content_hash(content: &str) -> String {
    sha256_hex(content)[..16].to_string()
}
```

**Upsert logic**:
```
1. Generate memory_id from (project_id, repo_id, file_path)
2. Compute content_hash from file content
3. Check if memory exists with same memory_id
4. If exists AND content_hash matches → skip (no change)
5. If exists AND content_hash differs → update memory + re-embed
6. If not exists → create new memory
```

This ensures:
- Same file always has same memory ID (idempotent)
- Content changes trigger re-indexing
- Deleted files can be detected (memory exists but file doesn't)
- No duplicate memories for same file

### Webhook Events Handled

| Event | Action |
|-------|--------|
| `push` | Index changed files + create commit summary |
| `pull_request` | Store PR as memory (title, desc, diff) |
| `pull_request.merged` | Update PR memory, link commits |

### Commit Processing Flow

When a push webhook arrives:

```
1. Parse commits from webhook payload
2. For each commit:
   a. Store commit metadata (sha, message, author, files)
   b. For each changed file:
      - Fetch file content via API
      - Upsert file memory (using content hash)
   c. Generate commit summary using LLM
   d. Store commit summary as searchable memory
```

### Auto-Generated Commit Summaries

For each commit (or batch of commits in a push), the LLM generates a summary:

```json
// Stored as memory type: "commit"
{
  "id": "mem_commit_abc123",
  "type": "commit",
  "title": "Add user authentication flow",
  "content": "## Summary\nThis commit implements JWT-based authentication...\n\n## Changes\n- Added `auth.rs` with login/logout handlers\n- Created `middleware/jwt.rs` for token validation\n- Updated `main.rs` to include auth routes\n\n## Impact\nUsers can now log in via the `/auth/login` endpoint...",
  "git_commit_sha": "abc123def456",
  "git_branch": "main",
  "author": "jane@example.com",
  "keywords": ["authentication", "jwt", "login", "security"],
  "metadata": {
    "files_changed": ["src/auth.rs", "src/middleware/jwt.rs", "src/main.rs"],
    "insertions": 245,
    "deletions": 12
  }
}
```

### LLM Prompt for Commit Summary

```
Given this git commit, create a concise technical summary:

Commit: {sha}
Author: {author}
Message: {commit_message}

Files changed:
{file_list_with_diff_stats}

Diffs (truncated):
{truncated_diffs}

Generate:
1. A clear title (max 60 chars)
2. A summary paragraph explaining what changed and why
3. Key changes as bullet points
4. Impact/implications if any
5. Relevant keywords for search
```

### Batch Commits (Multiple commits in one push)

When a push contains multiple commits:

```
Option A: Summarize each commit individually
  → More granular, but more LLM calls

Option B: Summarize the entire push as one (default)
  → Groups related work together
  → Single summary: "3 commits by jane: Add auth flow"
  → Links to individual commit SHAs
```

### File-Based Storage for Commit Summaries

Commit summaries are also stored as markdown files for easy access:

```
data/
├── fold.db
├── attachments/
└── summaries/
    └── {project_slug}/
        └── {repo_owner}-{repo_name}/
            └── 25/                    # Year
                └── 01/                # Month
                    ├── 15-14-32-abc123.md   # DD-HH-MM-{short_sha}.md
                    ├── 15-16-45-def456.md
                    └── 16-09-12-789abc.md
```

**File format**:
```markdown
---
sha: abc123def456789
author: jane@example.com
date: 2025-01-15T14:32:00Z
files: [src/auth.rs, src/middleware/jwt.rs, src/main.rs]
insertions: 245
deletions: 12
---

# Add user authentication flow

This commit implements JWT-based authentication with login/logout endpoints.

## Changes
- Added `auth.rs` with login/logout handlers
- Created `middleware/jwt.rs` for token validation
- Updated `main.rs` to include auth routes

## Impact
Users can now authenticate via `/auth/login`. Sessions expire after 7 days.
```

**Benefits**:
- Human-browseable without database
- Easy to back up / version control
- Can be served statically
- Grep-able for quick searches
- Memory record links to file path

### Benefits

- **Searchable history**: "What changed around authentication?" → finds commit summaries
- **Context for AI**: MCP can retrieve recent commits to understand recent work
- **Team awareness**: See what teammates have been working on
- **Debugging aid**: "When did we change the login flow?" → semantic search finds it
- **File access**: Browse summaries as markdown files in `data/summaries/`

### Metadata Repository Sync (Optional)

Projects can optionally sync generated metadata back to a git repository. This allows:
- Version-controlled summaries alongside code
- Easy sharing/browsing via GitHub/GitLab UI
- Backup of AI-generated insights
- Team access without Fold access

**Configuration Options**:

**Option A: Separate metadata repo**
```json
PUT /projects/{id}/metadata-repo
{
  "enabled": true,
  "mode": "separate",           // Dedicated repo for metadata
  "provider": "github",
  "owner": "myorg",
  "repo": "myproject-knowledge",
  "branch": "main",
  "access_token": "ghp_xxx",
  "path_prefix": ""             // Root of repo
}
```

**Option B: Same repo as source (recommended for most teams)**
```json
PUT /projects/{id}/metadata-repo
{
  "enabled": true,
  "mode": "in_repo",            // Store in source repo
  "repository_id": "repo_123",  // Which connected source repo to use
  "path_prefix": ".fold/"       // Folder in source repo (default: .fold/)
}
```

When using `in_repo` mode:
- Uses the same access token as the source repository
- Default path is `.fold/` (hidden folder, like `.github/`)
- Alternative: `docs/fold/` for visible documentation
- Indexer automatically ignores the metadata path (won't index as codebase)
- Webhook handler distinguishes: changes in `.fold/` → metadata sync, elsewhere → codebase index

**What Gets Synced** (all non-codebase memories):
| Memory Type | Path in Metadata Repo |
|-------------|----------------------|
| `commit` | `{prefix}commits/YY/MM/DD-HH-MM-{sha}.md` |
| `decision` | `{prefix}decisions/{slug}.md` |
| `spec` | `{prefix}specs/{slug}.md` |
| `session` | `{prefix}sessions/YY/MM/DD-{id}.md` |
| `task` | `{prefix}tasks/{slug}.md` |
| `general` | `{prefix}notes/{slug}.md` |
| `pr` | `{prefix}prs/{number}-{slug}.md` |

**Not synced**: `codebase` memories (these are just summaries of actual source files)

**Sync Flow**:
```
1. Memory created/updated (commit summary, decision, spec, session)
2. Check if project has metadata_repo configured + enabled
3. Generate markdown file content with frontmatter
4. Commit to metadata repo with message: "fold: Add/update {type} - {title}"
5. Push to configured branch
6. Store sync status on memory record
```

**File Format** (same as local, but in external repo):
```markdown
---
id: mem_abc123
type: commit
sha: abc123def456
author: jane@example.com
date: 2025-01-15T14:32:00Z
synced_at: 2025-01-15T14:35:00Z
---

# Add user authentication flow

This commit implements JWT-based authentication...
```

**Sync Status Tracking**:
```sql
-- Added to memories table
metadata_repo_synced_at TEXT,      -- NULL if never synced, timestamp if synced
metadata_repo_commit_sha TEXT,     -- Commit SHA in metadata repo
```

**Error Handling**:
- Sync failures logged but don't block main flow
- Retry on next memory update
- Status endpoint shows sync health per project

### Bidirectional Metadata Sync

The metadata repo can also be a **source** of memories. If someone adds or edits a markdown file directly in the metadata repo (via GitHub UI, IDE, etc.), Fold detects it and imports/updates the memory.

**How it works**:
```
1. Register webhook on metadata repo (same as source repos)
2. On push event → check which .md files changed
3. For each changed file:
   a. Parse frontmatter (id, type, etc.)
   b. If has `id` field → update existing memory
   c. If no `id` field → create new memory, write back with id
4. Re-embed content if changed
```

**File Detection Rules**:
| Path Pattern | Memory Type |
|--------------|-------------|
| `{prefix}commits/**/*.md` | `commit` (read-only, won't import) |
| `{prefix}decisions/*.md` | `decision` |
| `{prefix}specs/*.md` | `spec` |
| `{prefix}sessions/**/*.md` | `session` |
| `{prefix}tasks/*.md` | `task` |
| `{prefix}notes/*.md` | `general` |
| `{prefix}prs/*.md` | `pr` (read-only, won't import) |

**New file without frontmatter** (someone creates `decisions/use-redis.md`):
```markdown
# Use Redis for Caching

We decided to use Redis because...
```

Fold processes it:
1. Detects new file in `decisions/`
2. Creates memory with type=`decision`, title from `# heading`
3. Adds frontmatter and commits back:
```markdown
---
id: mem_xyz789
type: decision
created_at: 2025-01-15T10:00:00Z
synced_from: github
---

# Use Redis for Caching

We decided to use Redis because...
```

**Conflict Resolution**:
- If memory updated in Fold AND in repo since last sync → Fold wins (most recent)
- `synced_from` field tracks origin (`fold` or `github`/`gitlab`)
- Last-write-wins with timestamp comparison

**Read-only types**: `commit` and `pr` memories are auto-generated from source repo activity - editing them in metadata repo has no effect (they'd be overwritten on next commit/PR).

### Database Schema (Core)

```sql
-- Projects
projects (
  id TEXT PRIMARY KEY,
  slug TEXT UNIQUE NOT NULL,
  name TEXT NOT NULL,
  description TEXT,

  -- Optional: sync generated metadata to git
  metadata_repo_enabled INTEGER DEFAULT 0,
  metadata_repo_mode TEXT,            -- 'separate' | 'in_repo'
  -- For 'separate' mode:
  metadata_repo_provider TEXT,        -- 'github' | 'gitlab'
  metadata_repo_owner TEXT,
  metadata_repo_name TEXT,
  metadata_repo_branch TEXT,
  metadata_repo_token TEXT,           -- Encrypted
  -- For 'in_repo' mode:
  metadata_repo_source_id TEXT REFERENCES repositories(id),  -- Which source repo
  -- Shared:
  metadata_repo_path_prefix TEXT DEFAULT '.fold/',  -- Path in repo

  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
)

-- Memories (metadata - vectors stored in Qdrant)
memories (
  id TEXT PRIMARY KEY,            -- Deterministic for codebase files, UUID for manual
  project_id TEXT NOT NULL REFERENCES projects(id),
  repository_id TEXT REFERENCES repositories(id),  -- NULL for manual memories

  type TEXT NOT NULL,             -- 'codebase', 'session', 'spec', 'decision', 'task', 'general', 'commit', 'pr'
  title TEXT,
  content TEXT NOT NULL,          -- Full content for manual, summary for codebase/commit
  content_hash TEXT,              -- SHA256 prefix for change detection (codebase only)

  -- Source info (for codebase type)
  file_path TEXT,                 -- For codebase: source file path
  language TEXT,
  git_branch TEXT,
  git_commit_sha TEXT,

  -- For commit type: path to markdown file
  summary_file_path TEXT,         -- e.g., "summaries/myproj/org-repo/25/01/15-14-32-abc123.md"

  -- Metadata repo sync status
  metadata_repo_synced_at TEXT,   -- NULL if never synced
  metadata_repo_commit_sha TEXT,  -- Commit SHA in metadata repo
  metadata_repo_file_path TEXT,   -- Path in metadata repo
  synced_from TEXT,               -- 'fold' | 'github' | 'gitlab' (origin of last change)

  -- Metadata
  author TEXT,
  keywords TEXT,                  -- JSON array
  tags TEXT,                      -- JSON array

  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,

  -- For codebase: ensures one memory per file per repo
  UNIQUE(repository_id, file_path) WHERE repository_id IS NOT NULL AND type = 'codebase'
)

CREATE INDEX idx_memories_project ON memories(project_id);
CREATE INDEX idx_memories_type ON memories(project_id, type);
CREATE INDEX idx_memories_file ON memories(repository_id, file_path);
```

### Database Schema (Auth)

```sql
-- Users (created on first OIDC login)
users (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,          -- 'zitadel', 'google', 'github'
  subject TEXT NOT NULL,           -- 'sub' claim from OIDC
  email TEXT,
  display_name TEXT,
  avatar_url TEXT,
  role TEXT NOT NULL DEFAULT 'member',  -- 'admin' | 'member'
  created_at TEXT NOT NULL,
  last_login TEXT,
  UNIQUE(provider, subject)
)

-- Sessions (for web UI)
sessions (
  id TEXT PRIMARY KEY,             -- Session cookie value
  user_id TEXT NOT NULL REFERENCES users(id),
  expires_at TEXT NOT NULL,
  created_at TEXT NOT NULL
)

-- API Tokens (for MCP/programmatic access)
api_tokens (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id),
  name TEXT NOT NULL,              -- User-provided description
  token_hash TEXT NOT NULL,        -- SHA256 of token
  token_prefix TEXT NOT NULL,      -- First 8 chars for identification
  project_ids TEXT NOT NULL,       -- JSON array of project IDs
  created_at TEXT NOT NULL,
  last_used TEXT,
  expires_at TEXT                  -- Optional expiry
)
```

### Database Schema (Git)

```sql
-- For each connected repo (single branch)
repositories (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  provider TEXT NOT NULL,         -- 'github' | 'gitlab'
  owner TEXT NOT NULL,
  repo TEXT NOT NULL,
  branch TEXT NOT NULL,           -- Single branch to monitor (e.g., 'main')

  -- Webhook
  webhook_id TEXT,                -- To delete on disconnect
  webhook_secret TEXT,            -- To verify payloads

  -- Auth
  access_token TEXT NOT NULL,     -- Encrypted

  -- Status
  last_indexed_at TEXT,
  last_commit_sha TEXT,           -- Last processed commit
  created_at TEXT NOT NULL,

  UNIQUE(project_id, provider, owner, repo, branch)  -- Can add same repo with different branch
)

-- Raw commit data
git_commits (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id),
  sha TEXT NOT NULL,
  message TEXT NOT NULL,
  author_name TEXT,
  author_email TEXT,
  files_changed TEXT,             -- JSON array of {path, status, additions, deletions}
  insertions INTEGER,
  deletions INTEGER,
  committed_at TEXT NOT NULL,
  indexed_at TEXT NOT NULL,
  summary_memory_id TEXT REFERENCES memories(id),  -- Links to LLM-generated summary
  UNIQUE(repository_id, sha)
)

-- PRs as memories
git_pull_requests (
  id, repository_id,
  number, title, description, state,
  author, source_branch, target_branch,
  created_at, merged_at, indexed_at
)
```

### Database Schema (AI Sessions)

```sql
-- AI agent working sessions
ai_sessions (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),

  -- What the agent is working on
  task TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active',  -- 'active', 'paused', 'completed', 'blocked'

  -- Local workspace mapping
  local_root TEXT,                -- e.g., "/Users/frank/projects/myapp"
  repository_id TEXT REFERENCES repositories(id),

  -- Session data
  summary TEXT,                   -- Final summary when session ends
  next_steps TEXT,                -- JSON array of suggested next steps

  -- Tracking
  agent_type TEXT,                -- 'claude-code', 'cursor', 'custom', etc.
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  ended_at TEXT
)

-- Notes/findings during a session
ai_session_notes (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES ai_sessions(id) ON DELETE CASCADE,
  type TEXT NOT NULL,             -- 'decision', 'blocker', 'question', 'progress', 'finding'
  content TEXT NOT NULL,
  created_at TEXT NOT NULL
)

-- Workspace mappings (for local path resolution)
workspaces (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  token_id TEXT NOT NULL REFERENCES api_tokens(id),  -- Which token registered this
  local_root TEXT NOT NULL,       -- Local directory path
  repository_id TEXT REFERENCES repositories(id),
  created_at TEXT NOT NULL,
  expires_at TEXT                 -- Auto-cleanup old mappings
)
```

---

## File Structure

```
fold/
├── Cargo.toml
├── .env.example
├── migrations/
│   ├── 001_initial.sql
│   └── 002_repositories.sql
│
├── src/
│   ├── main.rs              # Entry point, server setup
│   ├── lib.rs               # Re-exports
│   ├── config.rs            # Env vars, provider config
│   ├── error.rs             # Error types (thiserror)
│   │
│   ├── models/
│   │   ├── mod.rs
│   │   ├── memory.rs        # Memory, MemoryType
│   │   ├── project.rs       # Project
│   │   ├── user.rs          # User (admin), ApiToken
│   │   ├── repository.rs    # Repository, GitCommit, GitPR
│   │   ├── team.rs          # TeamStatus
│   │   └── session.rs       # AiSession, SessionNote, Workspace
│   │
│   ├── db/
│   │   ├── mod.rs
│   │   ├── pool.rs          # SQLite connection pool
│   │   ├── users.rs         # User/token queries
│   │   ├── projects.rs      # Project queries
│   │   ├── memories.rs      # Memory metadata queries
│   │   ├── links.rs         # Memory link queries
│   │   ├── attachments.rs   # Attachment queries
│   │   ├── repositories.rs  # Repo/commit/PR queries
│   │   ├── sessions.rs      # AI session queries
│   │   └── qdrant.rs        # Qdrant client wrapper
│   │
│   ├── services/
│   │   ├── mod.rs
│   │   ├── auth.rs          # OIDC provider handling, session management
│   │   ├── memory.rs        # Memory CRUD + search
│   │   ├── project.rs       # Project management
│   │   ├── attachments.rs   # File upload/download
│   │   ├── embeddings.rs    # fastembed wrapper
│   │   ├── llm.rs           # Multi-provider with fallback
│   │   ├── indexer.rs       # File content indexer
│   │   ├── github.rs        # GitHub API client
│   │   ├── gitlab.rs        # GitLab API client
│   │   └── git_sync.rs      # Webhook processing, indexing
│   │
│   ├── api/
│   │   ├── mod.rs           # Router setup
│   │   ├── auth.rs          # Login, token management
│   │   ├── projects.rs      # Project CRUD
│   │   ├── memories.rs      # Memory endpoints
│   │   ├── attachments.rs   # File upload/download endpoints
│   │   ├── repositories.rs  # Repo connection endpoints
│   │   ├── search.rs        # Search endpoints
│   │   ├── team.rs          # Team status
│   │   ├── mcp.rs           # MCP JSON-RPC handler
│   │   └── webhooks.rs      # GitHub/GitLab webhooks
│   │
│   └── middleware/
│       ├── mod.rs
│       ├── token_auth.rs    # API token extraction
│       └── admin_auth.rs    # JWT validation
│
├── tests/
│   └── integration.rs
│
└── Dockerfile
```

**~38 files total**

---

## API Endpoints

### Auth (OIDC)
```
GET  /auth/providers              # List configured providers (for UI login buttons)
GET  /auth/login/{provider}       # Redirect to provider's auth URL
GET  /auth/callback/{provider}    # OAuth callback → create session → redirect to UI
POST /auth/logout                 # Clear session cookie
GET  /auth/me                     # Current user info, role, accessible projects
```

### Admin (requires admin role)
```
GET    /admin/users               # List all users
PUT    /admin/users/{id}/role     # Change user role (admin/member)
DELETE /admin/users/{id}          # Remove user

GET    /admin/tokens              # List all API tokens (all users)
DELETE /admin/tokens/{id}         # Revoke any token
```

### User Token Management (authenticated users)
```
GET    /me/tokens                 # List my API tokens
POST   /me/tokens                 # Create API token (scoped to my accessible projects)
DELETE /me/tokens/{id}            # Revoke my token
```

### Projects
```
GET    /projects              # List (filtered by token scope)
POST   /projects              # Create (admin only)
GET    /projects/{id}         # Get details
PUT    /projects/{id}         # Update
DELETE /projects/{id}         # Delete

GET    /projects/{id}/stats   # Memory counts, last activity
```

### Content Sources

Fold supports multiple content source types (Git now, cloud storage planned):

```rust
// Source provider trait (for future extensibility)
trait ContentSource {
    fn list_files(&self) -> Vec<FileInfo>;
    fn get_file(&self, path: &str) -> FileContent;
    fn watch_changes(&self) -> ChangeStream;  // Webhooks for git, polling for cloud
}

// Implemented providers
- GitHubProvider    ✅ Implemented
- GitLabProvider    ✅ Implemented
- GoogleDriveProvider   🔮 Future
- DropboxProvider       🔮 Future
- S3Provider            🔮 Future
- LocalFSProvider       🔮 Future (for self-hosted)
```

### Repositories (Git - Source Code)
```
GET    /projects/{id}/repositories           # List connected repos
POST   /projects/{id}/repositories           # Connect repo (triggers initial index)
DELETE /projects/{id}/repositories/{rid}     # Disconnect repo

POST   /projects/{id}/repositories/{rid}/reindex  # Force full reindex
GET    /projects/{id}/repositories/{rid}/commits  # List indexed commits
GET    /projects/{id}/repositories/{rid}/prs      # List indexed PRs
```

### Historical Commit Indexing

When connecting a repo, you can index historical commits:

```json
POST /projects/{id}/repositories
{
  "provider": "github",
  "owner": "myorg",
  "repo": "myproject",
  "branch": "main",
  "access_token": "ghp_xxx",

  // Historical indexing options
  "history": {
    "enabled": true,
    "mode": "days",           // 'days', 'commits', 'since_date', 'since_commit'
    "value": 30               // Last 30 days
  }
}

// Alternative modes:
{ "mode": "commits", "value": 100 }           // Last 100 commits
{ "mode": "since_date", "value": "2025-01-01" }
{ "mode": "since_commit", "value": "abc123" } // From specific commit forward
```

**What happens**:
1. Fetch current HEAD and index all files (creates codebase memories)
2. Walk commit history back to the specified point
3. For each historical commit, create a commit summary memory
4. Link commits to files they modified
5. Process any PRs in that time range

**API to trigger historical indexing on existing repo**:
```
POST /projects/{id}/repositories/{rid}/index-history
{
  "mode": "days",
  "value": 30
}
```

**Job tracking**: Historical indexing runs as a background job with progress:
```json
GET /status/jobs/{job_id}
{
  "type": "index_history",
  "status": "running",
  "progress": {
    "total_commits": 150,
    "processed": 87,
    "percent": 58
  }
}
```

### Metadata Repository (Knowledge Sync)
```
GET    /projects/{id}/metadata-repo          # Get metadata repo config
PUT    /projects/{id}/metadata-repo          # Configure metadata repo
DELETE /projects/{id}/metadata-repo          # Disconnect metadata repo

POST   /projects/{id}/metadata-repo/sync     # Force bidirectional sync
GET    /projects/{id}/metadata-repo/status   # Sync health, last sync time, pending items
```

### Memories
```
GET    /projects/{id}/memories         # List (with filters)
POST   /projects/{id}/memories         # Add memory
GET    /projects/{id}/memories/{mid}   # Get single
PUT    /projects/{id}/memories/{mid}   # Update memory
DELETE /projects/{id}/memories/{mid}   # Delete

POST   /projects/{id}/search           # Semantic search
POST   /projects/{id}/context          # Get context for task

# Links
POST   /projects/{id}/memories/{mid}/links           # Add link
GET    /projects/{id}/memories/{mid}/links           # List links
DELETE /projects/{id}/memories/{mid}/links/{lid}     # Remove link

# AI-suggested links
GET    /projects/{id}/memories/{mid}/suggested-links              # Get suggestions
POST   /projects/{id}/memories/{mid}/suggested-links/{sid}/approve
POST   /projects/{id}/memories/{mid}/suggested-links/{sid}/reject
```

### Knowledge Graph
```
GET    /projects/{id}/graph/neighbors/{mid}          # Direct connections
GET    /projects/{id}/graph/cluster/{mid}            # Cluster around memory (depth param)
GET    /projects/{id}/graph/path                     # Path between memories (?from=&to=)
GET    /projects/{id}/graph/subgraph                 # Subgraph for memory set (?ids=)

GET    /projects/{id}/graph/context/{mid}            # Rich context (decisions, specs, etc.)
GET    /projects/{id}/graph/history/{file_path}      # File history (commits, PRs, decisions)
GET    /projects/{id}/graph/impact/{mid}             # Impact analysis
```

### Attachments
```
GET    /projects/{id}/memories/{mid}/attachments      # List attachments
POST   /projects/{id}/memories/{mid}/attachments      # Upload attachment
GET    /projects/{id}/memories/{mid}/attachments/{aid} # Download file
DELETE /projects/{id}/memories/{mid}/attachments/{aid} # Delete attachment
```

### Team
```
GET    /projects/{id}/team             # Team status
POST   /projects/{id}/team/status      # Update my status
```

### AI Sessions
```
POST   /projects/{id}/sessions                    # Start session (returns context)
GET    /projects/{id}/sessions/{sid}              # Get session details
POST   /projects/{id}/sessions/{sid}/notes        # Add note to session
PUT    /projects/{id}/sessions/{sid}              # Update session (pause/complete)
GET    /projects/{id}/sessions                    # List recent sessions

POST   /projects/{id}/workspace                   # Register local workspace mapping
DELETE /projects/{id}/workspace                   # Clear workspace mapping
```

### Webhooks (called by GitHub/GitLab)
```
POST   /webhooks/github/{repo_id}      # GitHub webhook receiver
POST   /webhooks/gitlab/{repo_id}      # GitLab webhook receiver
```

### Monitoring & Status
```
GET    /health                         # Basic health check (for load balancers)
GET    /health/ready                   # Readiness (DB + Qdrant connected)
GET    /health/live                    # Liveness (server responding)

GET    /status                         # System overview (authenticated)
GET    /status/jobs                    # List active/recent indexing jobs
GET    /status/jobs/{id}               # Specific job progress
GET    /status/jobs/{id}/logs          # Job log stream (SSE)

GET    /metrics                        # Prometheus metrics (optional)
```

### MCP
```
POST   /mcp                            # JSON-RPC endpoint
GET    /mcp/sse                        # SSE transport (optional)
```

---

## AI Agent Integration

Fold is designed to be the **holographic memory layer** for AI agents (Claude Code, Cursor, Copilot, custom agents).

### Local Path Resolution

When an agent works in a local directory, Fold can map memories back to local files:

**Agent registers local context**:
```json
// MCP tool: register_workspace
{
  "project": "myapp",
  "local_root": "/Users/frank/projects/myapp",  // Where repo is cloned
  "repository_id": "repo_123"                    // Optional: which repo this maps to
}
```

**Search results include local paths**:
```json
// MCP tool: memory_search returns:
{
  "memories": [
    {
      "id": "mem_auth_rs",
      "type": "codebase",
      "title": "Authentication module",
      "content": "Handles JWT validation and session management...",
      "file_path": "src/auth.rs",                    // Path in repo
      "local_path": "/Users/frank/projects/myapp/src/auth.rs",  // Resolved local path
      "repository": {
        "id": "repo_123",
        "provider": "github",
        "full_name": "myorg/myapp"
      }
    }
  ]
}
```

**Why this matters**: Claude Code can search Fold, get results, and directly open/read the local files.

### Session Handoff (For Planning)

When an AI agent starts working, it can:

1. **Load context** - Get relevant memories for the task
2. **Register session** - Track what it's working on
3. **Save progress** - Store notes, decisions, blockers
4. **Handoff** - Pass context to next agent/session

**Starting a session**:
```json
// MCP tool: session_start
{
  "project": "myapp",
  "task": "Implement user authentication with JWT",
  "local_root": "/Users/frank/projects/myapp"
}

// Returns:
{
  "session_id": "sess_abc123",
  "context": {
    "relevant_files": [
      { "path": "src/auth.rs", "local_path": "...", "summary": "..." },
      { "path": "src/middleware/jwt.rs", "local_path": "...", "summary": "..." }
    ],
    "relevant_decisions": [
      { "id": "mem_dec_jwt", "title": "Use JWT for sessions", "content": "..." }
    ],
    "relevant_specs": [...],
    "recent_commits": [
      { "sha": "abc123", "title": "Add login endpoint", "files": [...] }
    ],
    "team_activity": [
      { "user": "jane", "task": "Working on OAuth integration", "files": [...] }
    ]
  }
}
```

**During the session**:
```json
// MCP tool: session_note
{
  "session_id": "sess_abc123",
  "note": "Decided to use RS256 for JWT signing. Need to add key rotation later.",
  "type": "decision"  // or "blocker", "question", "progress"
}
```

**Ending/pausing session**:
```json
// MCP tool: session_end
{
  "session_id": "sess_abc123",
  "summary": "Implemented JWT auth. Login/logout working. TODO: add refresh tokens.",
  "status": "paused",  // or "completed", "blocked"
  "next_steps": ["Add refresh token endpoint", "Add key rotation"]
}
```

### General Knowledge Base (Non-Code)

Fold isn't just for code - it's a **holographic knowledge base** for any project:

**Project types**:
```json
POST /projects
{
  "name": "Product Documentation",
  "type": "knowledge",           // vs "codebase" (default)
  "description": "Internal docs, decisions, specs"
}
```

**Use cases beyond code**:
- Research notes and papers
- Meeting notes and decisions
- Product specs and requirements
- Customer feedback synthesis
- Competitor analysis
- Internal wikis
- Learning/training materials

**Memory types work for any domain**:
| Type | Code Context | Knowledge Context |
|------|--------------|-------------------|
| `spec` | Feature specification | Product requirements |
| `decision` | Architecture decision | Business decision |
| `session` | Coding session notes | Meeting notes |
| `task` | Development task | Action item |
| `general` | Notes | Any knowledge |

### AI-Optimized Responses

All API/MCP responses are structured for LLM consumption:

**Principles**:
1. **Concise but complete** - Summaries first, details on request
2. **Structured data** - JSON with consistent schema
3. **Relationship context** - Always include how things connect
4. **Action hints** - Suggest what the agent might do next

**Example: context_get response**:
```json
// MCP tool: context_get
{
  "task": "Fix the login bug where sessions expire too early",
  "project": "myapp"
}

// Returns (AI-optimized):
{
  "summary": "Found 3 relevant files and 2 recent changes related to session handling.",

  "files": [
    {
      "path": "src/auth.rs",
      "local_path": "/Users/frank/projects/myapp/src/auth.rs",
      "relevance": 0.95,
      "summary": "JWT validation, session creation. Line 45-60 handles expiry.",
      "hint": "Check SESSION_EXPIRY constant and validate_token() function"
    }
  ],

  "recent_changes": [
    {
      "type": "commit",
      "date": "2 days ago",
      "title": "Reduce session timeout for security",
      "author": "jane",
      "relevance": 0.92,
      "hint": "This commit changed SESSION_EXPIRY from 7 days to 1 hour - likely the cause"
    }
  ],

  "decisions": [
    {
      "title": "Session timeout policy",
      "summary": "Sessions should expire after 7 days of inactivity",
      "hint": "Recent commit contradicts this decision - may be a mistake"
    }
  ],

  "suggested_actions": [
    "Read src/auth.rs lines 45-60",
    "Check commit abc123 for the timeout change",
    "Verify intended session expiry with team"
  ]
}
```

### MCP Tool Design Philosophy

Tools are designed for how agents actually work:

| Agent Need | MCP Tool | What It Does |
|------------|----------|--------------|
| "What do I need to know?" | `context_get` | Returns relevant context for a task |
| "What files relate to X?" | `codebase_search` | Semantic search with local paths |
| "What changed recently?" | `git_commits` | Recent commits with summaries |
| "Who's working on what?" | `team_status_view` | Team activity |
| "Remember this for later" | `memory_add` | Store a note/decision/finding |
| "What did we decide about X?" | `memory_search` | Find decisions/specs |
| "How does A relate to B?" | `graph_context` | Relationship map |
| "I'm starting work" | `session_start` | Load context, register session |
| "I found something" | `session_note` | Add note to current session |
| "I'm done for now" | `session_end` | Save session, suggest handoff |

---

## MCP Tools

```
project_list          # List projects (token-scoped)
project_get           # Get project details

memory_add            # Add a memory
memory_list           # List memories
memory_search         # Semantic search
memory_update         # Update a memory
memory_delete         # Delete a memory

attachment_upload     # Upload file to memory
attachment_list       # List attachments on memory
attachment_get        # Get attachment metadata

context_get           # Get relevant context for a task

codebase_search       # Search indexed code

git_commits           # List recent commits
git_prs               # List PRs

# Graph/Relationships
memory_link_add       # Add link between memories
memory_link_list      # List links for a memory
graph_context         # Rich context around a memory (files, decisions, specs)
graph_history         # History of a file (commits, PRs, decisions)
graph_related         # Find related memories (semantic + structural)
graph_impact          # What would be affected by changes
graph_path            # Find connection path between two memories

team_status_view      # See team activity
team_status_update    # Update my status

# Workspace/Session (for AI agents)
register_workspace    # Map local directory to project/repo
session_start         # Start working session, get context
session_note          # Add note during session
session_end           # End session with summary

metadata_sync_status  # Check sync status with metadata repo
metadata_sync_trigger # Trigger manual sync
```

---

## Implementation Phases

### Phase 1: Foundation
```
1. Cargo.toml + dependencies
2. src/main.rs (minimal axum server)
3. src/config.rs (env loading)
4. src/error.rs (error types)
5. src/models/*.rs (all models)
6. migrations/*.sql (schema)
7. src/db/pool.rs (SQLite pool)
```

### Phase 2: Database Layer
```
8.  src/db/users.rs
9.  src/db/projects.rs
10. src/db/memories.rs
11. src/db/repositories.rs
12. src/db/qdrant.rs
```

### Phase 3: Core Services
```
13. src/services/embeddings.rs (fastembed)
14. src/services/llm.rs (multi-provider fallback)
15. src/services/memory.rs
16. src/services/project.rs
17. src/services/indexer.rs
```

### Phase 4: REST API
```
18. src/middleware/token_auth.rs
19. src/middleware/admin_auth.rs
20. src/api/auth.rs
21. src/api/projects.rs
22. src/api/memories.rs
23. src/api/search.rs
24. src/api/team.rs
25. src/api/mod.rs (router)
```

### Phase 5: Git Integration
```
26. src/services/github.rs
27. src/services/gitlab.rs
28. src/services/git_sync.rs
29. src/api/repositories.rs
30. src/api/webhooks.rs
```

### Phase 6: MCP
```
31. src/api/mcp.rs
```

### Phase 7: Polish
```
32. src/main.rs (complete wiring)
33. src/lib.rs
34. .env.example
35. Dockerfile
36. Compile + test
```

---

## Environment Variables

```bash
# Server
HOST=0.0.0.0
PORT=8765
PUBLIC_URL=https://fold.example.com  # For webhook callbacks

# Database
DATABASE_PATH=./data/fold.db
QDRANT_URL=http://localhost:6334

# Embeddings
EMBEDDING_MODEL=sentence-transformers/all-MiniLM-L6-v2

# LLM Providers (in fallback order)
GOOGLE_API_KEY=           # Gemini (free tier) - priority 1
OPENROUTER_API_KEY=       # OpenRouter - priority 2
OPENAI_API_KEY=           # OpenAI - priority 3

# Auth Providers (add as many as needed)
# Pattern: AUTH_PROVIDER_{NAME}_{FIELD}
AUTH_PROVIDER_CORPORATE_TYPE=oidc
AUTH_PROVIDER_CORPORATE_DISPLAY_NAME=Corporate SSO
AUTH_PROVIDER_CORPORATE_ISSUER=https://auth.company.com
AUTH_PROVIDER_CORPORATE_CLIENT_ID=
AUTH_PROVIDER_CORPORATE_CLIENT_SECRET=

# Example: Google
# AUTH_PROVIDER_GOOGLE_TYPE=oidc
# AUTH_PROVIDER_GOOGLE_DISPLAY_NAME=Google
# AUTH_PROVIDER_GOOGLE_ISSUER=https://accounts.google.com
# AUTH_PROVIDER_GOOGLE_CLIENT_ID=
# AUTH_PROVIDER_GOOGLE_CLIENT_SECRET=

# Example: GitHub (OAuth2)
# AUTH_PROVIDER_GITHUB_TYPE=github
# AUTH_PROVIDER_GITHUB_DISPLAY_NAME=GitHub
# AUTH_PROVIDER_GITHUB_CLIENT_ID=
# AUTH_PROVIDER_GITHUB_CLIENT_SECRET=

# Admin Bootstrap (one-time, remove after first admin created)
ADMIN_BOOTSTRAP_TOKEN=    # Required to create first admin

# Session
SESSION_SECRET=           # Random 32 bytes for cookie signing

# Git Integration (for repo API access, separate from auth)
GITHUB_APP_ID=            # Optional: GitHub App for better rate limits
GITHUB_APP_PRIVATE_KEY=   # Optional: GitHub App private key

# Webhook Secrets (auto-generated per repo, stored in DB)
```

---

## Verification Checklist

After implementation:

- [ ] `cargo build --release` succeeds
- [ ] `docker run -p 6334:6334 qdrant/qdrant` (start Qdrant)
- [ ] `cargo run` starts server on :8765
- [ ] Admin login works, returns JWT
- [ ] Create API token scoped to project
- [ ] Create project via admin API
- [ ] Connect GitHub repo (webhook registered)
- [ ] Push to repo → webhook fires → files indexed
- [ ] Search memories returns results
- [ ] MCP tools work in Claude Desktop
- [ ] Token without project access gets 403

---

## Testing Strategy

### Test-First Approach

Every module gets tests BEFORE implementation:

```
tests/
├── common/
│   ├── mod.rs              # Shared test utilities
│   ├── fixtures.rs         # Test data factories
│   └── mocks.rs            # Mock services (LLM, GitHub, etc.)
│
├── unit/
│   ├── models_test.rs      # Model serialization, validation
│   ├── embeddings_test.rs  # Embedding generation
│   ├── llm_test.rs         # LLM fallback logic
│   └── auth_test.rs        # Token validation, session logic
│
├── integration/
│   ├── db_test.rs          # SQLite operations
│   ├── qdrant_test.rs      # Vector operations
│   ├── memory_test.rs      # Full memory CRUD cycle
│   ├── search_test.rs      # Search accuracy
│   ├── indexer_test.rs     # File indexing
│   └── git_sync_test.rs    # Webhook processing
│
└── api/
    ├── auth_test.rs        # OIDC flow, session, tokens
    ├── projects_test.rs    # Project CRUD
    ├── memories_test.rs    # Memory endpoints
    ├── search_test.rs      # Search endpoints
    ├── webhooks_test.rs    # Webhook signature validation
    └── mcp_test.rs         # MCP protocol compliance
```

### Test Commands

```bash
# Run all tests
cargo test

# Run with coverage
cargo tarpaulin --out Html

# Run specific test module
cargo test --test integration::memory_test

# Run tests matching pattern
cargo test search

# Run with logging
RUST_LOG=debug cargo test -- --nocapture
```

### Test Fixtures

```rust
// tests/common/fixtures.rs

pub fn test_project() -> Project { ... }
pub fn test_memory(project_id: &str) -> Memory { ... }
pub fn test_user() -> User { ... }
pub fn test_api_token(user_id: &str) -> ApiToken { ... }
```

### Mocking External Services

```rust
// tests/common/mocks.rs

pub struct MockLlmService { responses: Vec<String> }
pub struct MockGitHubClient { webhooks: Vec<WebhookEvent> }
pub struct MockQdrantClient { vectors: HashMap<String, Vec<f32>> }
```

### CI Pipeline

```yaml
# .github/workflows/test.yml
- cargo fmt --check
- cargo clippy -- -D warnings
- cargo test
- cargo tarpaulin --out Xml  # Coverage report
```

---

## Job Queue & Monitoring

### Background Jobs

Indexing operations run as background jobs with progress tracking:

```sql
-- Job tracking table
jobs (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,           -- 'index_repo', 'reindex_repo', 'index_files'
  status TEXT NOT NULL,         -- 'pending', 'running', 'completed', 'failed'
  project_id TEXT,
  repository_id TEXT,

  -- Progress
  total_items INTEGER,
  processed_items INTEGER,
  failed_items INTEGER,

  -- Timing
  created_at TEXT NOT NULL,
  started_at TEXT,
  completed_at TEXT,

  -- Results
  result TEXT,                  -- JSON with summary
  error TEXT                    -- Error message if failed
)

-- Job logs (for streaming)
job_logs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  job_id TEXT NOT NULL REFERENCES jobs(id),
  level TEXT NOT NULL,          -- 'info', 'warn', 'error'
  message TEXT NOT NULL,
  metadata TEXT,                -- JSON
  created_at TEXT NOT NULL
)
```

### Job Status API Response

```json
GET /status/jobs/job_123

{
  "id": "job_123",
  "type": "index_repo",
  "status": "running",
  "project": { "id": "proj_1", "name": "my-project" },
  "repository": { "id": "repo_1", "name": "owner/repo" },
  "progress": {
    "total": 150,
    "processed": 87,
    "failed": 2,
    "percent": 58
  },
  "timing": {
    "created_at": "2024-01-15T10:00:00Z",
    "started_at": "2024-01-15T10:00:05Z",
    "elapsed_seconds": 45,
    "estimated_remaining_seconds": 32
  },
  "recent_logs": [
    { "level": "info", "message": "Indexed src/main.rs", "at": "..." },
    { "level": "warn", "message": "Skipped large file: data.json", "at": "..." }
  ]
}
```

### Real-time Log Streaming

```
GET /status/jobs/{id}/logs
Accept: text/event-stream

data: {"level":"info","message":"Starting index...","at":"..."}
data: {"level":"info","message":"Indexed src/lib.rs","at":"..."}
data: {"level":"info","message":"Indexed src/main.rs","at":"..."}
```

### Metrics (Prometheus)

```
GET /metrics

# HELP fold_memories_total Total memories by type
fold_memories_total{project="my-proj",type="codebase"} 1234
fold_memories_total{project="my-proj",type="decision"} 45

# HELP fold_search_requests_total Search requests
fold_search_requests_total{project="my-proj"} 5678

# HELP fold_index_jobs_total Indexing jobs by status
fold_index_jobs_total{status="completed"} 100
fold_index_jobs_total{status="failed"} 3

# HELP fold_llm_requests_total LLM requests by provider
fold_llm_requests_total{provider="gemini"} 500
fold_llm_requests_total{provider="openrouter"} 50
```

---

## Updated File Structure

```
fold/
├── Cargo.toml
├── Cargo.lock
├── .env.example
├── .gitignore
├── docker-compose.yml
├── Dockerfile
├── README.md
├── migrations/
│   ├── 001_initial.sql
│   ├── 002_repositories.sql
│   └── 003_jobs.sql
│
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config.rs
│   ├── error.rs
│   │
│   ├── models/
│   │   ├── mod.rs
│   │   ├── memory.rs
│   │   ├── project.rs
│   │   ├── user.rs
│   │   ├── repository.rs
│   │   ├── team.rs
│   │   └── job.rs            # NEW: Job, JobLog
│   │
│   ├── db/
│   │   ├── mod.rs
│   │   ├── pool.rs
│   │   ├── users.rs
│   │   ├── projects.rs
│   │   ├── memories.rs
│   │   ├── attachments.rs
│   │   ├── repositories.rs
│   │   ├── jobs.rs           # NEW: Job queries
│   │   └── qdrant.rs
│   │
│   ├── services/
│   │   ├── mod.rs
│   │   ├── auth.rs
│   │   ├── memory.rs
│   │   ├── project.rs
│   │   ├── attachments.rs
│   │   ├── embeddings.rs
│   │   ├── llm.rs
│   │   ├── indexer.rs
│   │   ├── github.rs
│   │   ├── gitlab.rs
│   │   ├── git_sync.rs
│   │   ├── metadata_sync.rs  # Sync memories to metadata repo
│   │   ├── graph.rs          # Knowledge graph queries & traversal
│   │   ├── linker.rs         # Auto-generate links (commit→files, etc.)
│   │   ├── sessions.rs       # AI session management
│   │   └── jobs.rs           # Job runner, queue
│   │
│   ├── api/
│   │   ├── mod.rs
│   │   ├── auth.rs
│   │   ├── projects.rs
│   │   ├── memories.rs
│   │   ├── attachments.rs
│   │   ├── repositories.rs
│   │   ├── search.rs
│   │   ├── graph.rs          # Knowledge graph endpoints
│   │   ├── sessions.rs       # AI session endpoints
│   │   ├── team.rs
│   │   ├── mcp.rs
│   │   ├── webhooks.rs
│   │   └── status.rs         # Health, jobs, metrics
│   │
│   └── middleware/
│       ├── mod.rs
│       ├── token_auth.rs
│       └── session_auth.rs
│
├── tests/
│   ├── common/
│   │   ├── mod.rs
│   │   ├── fixtures.rs
│   │   └── mocks.rs
│   ├── unit/
│   │   ├── mod.rs
│   │   ├── models_test.rs
│   │   ├── embeddings_test.rs
│   │   ├── llm_test.rs
│   │   └── auth_test.rs
│   ├── integration/
│   │   ├── mod.rs
│   │   ├── db_test.rs
│   │   ├── qdrant_test.rs
│   │   ├── memory_test.rs
│   │   ├── search_test.rs
│   │   ├── indexer_test.rs
│   │   └── git_sync_test.rs
│   └── api/
│       ├── mod.rs
│       ├── auth_test.rs
│       ├── projects_test.rs
│       ├── memories_test.rs
│       ├── search_test.rs
│       ├── webhooks_test.rs
│       └── mcp_test.rs
```

**~65 files total** (including tests + docker)

---

## Docker Compose (Default)

### docker-compose.yml

```yaml
services:
  fold:
    build: .
    ports:
      - "8765:8765"
    environment:
      - HOST=0.0.0.0
      - PORT=8765
      - DATABASE_PATH=/data/fold.db
      - QDRANT_URL=http://qdrant:6334
      - ATTACHMENTS_PATH=/data/attachments
      - SUMMARIES_PATH=/data/summaries
      # Auth providers (from .env)
      # Auth providers (pass through from .env)
      # Add as many AUTH_PROVIDER_{NAME}_{FIELD} vars as needed
      # LLM providers (from .env)
      - GOOGLE_API_KEY
      - OPENROUTER_API_KEY
      - OPENAI_API_KEY
    volumes:
      - fold-data:/data
      - fastembed-cache:/root/.cache/fastembed  # Embedding model cache
    depends_on:
      qdrant:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8765/health"]
      interval: 10s
      timeout: 5s
      retries: 3

  qdrant:
    image: qdrant/qdrant:latest
    ports:
      - "6333:6333"   # REST API
      - "6334:6334"   # gRPC
    volumes:
      - qdrant-data:/qdrant/storage
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:6333/healthz"]
      interval: 5s
      timeout: 3s
      retries: 5

volumes:
  fold-data:
  qdrant-data:
  fastembed-cache:
```

### Dockerfile

```dockerfile
# Build stage
FROM rust:1.75-bookworm as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build release binary
RUN cargo build --release

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

# Create data directory
RUN mkdir -p /data/attachments

EXPOSE 8765

CMD ["/app/fold"]
```

### Running

```bash
# Start everything
docker-compose up -d

# View logs
docker-compose logs -f fold

# Stop
docker-compose down

# Reset data (careful!)
docker-compose down -v
```

### Local Development (without Docker)

```bash
# Start just Qdrant
docker-compose up -d qdrant

# Run Fold locally
cargo run

# Or with auto-reload
cargo watch -x run
```

### Environment File (.env)

```bash
# .env (copy from .env.example)

# Server
HOST=0.0.0.0
PORT=8765
PUBLIC_URL=http://localhost:8765

# Database (local dev paths)
DATABASE_PATH=./data/fold.db
QDRANT_URL=http://localhost:6334
ATTACHMENTS_PATH=./data/attachments
SUMMARIES_PATH=./data/summaries

# Auth Providers - add as many as needed
# Pattern: AUTH_PROVIDER_{NAME}_{FIELD}

# Example: Corporate OIDC
AUTH_PROVIDER_CORPORATE_TYPE=oidc
AUTH_PROVIDER_CORPORATE_DISPLAY_NAME=Corporate SSO
AUTH_PROVIDER_CORPORATE_ISSUER=https://auth.company.com
AUTH_PROVIDER_CORPORATE_CLIENT_ID=
AUTH_PROVIDER_CORPORATE_CLIENT_SECRET=

# Example: GitHub OAuth
# AUTH_PROVIDER_GITHUB_TYPE=github
# AUTH_PROVIDER_GITHUB_DISPLAY_NAME=GitHub
# AUTH_PROVIDER_GITHUB_CLIENT_ID=
# AUTH_PROVIDER_GITHUB_CLIENT_SECRET=

# LLM Providers (in fallback order)
GOOGLE_API_KEY=           # Gemini - priority 1
OPENROUTER_API_KEY=       # OpenRouter - priority 2
OPENAI_API_KEY=           # OpenAI - priority 3

# Session
SESSION_SECRET=change-this-to-random-32-bytes
```

---

## Notes

- fastembed downloads model on first run (~90MB) - cached in volume
- Qdrant must be healthy before Fold starts (docker-compose handles this)
- Webhook URLs need to be publicly accessible (use ngrok for local dev)
- GitHub/GitLab tokens stored encrypted in SQLite
- First admin requires `ADMIN_BOOTSTRAP_TOKEN` (no auto-promote)
- Jobs table enables progress tracking and resumable indexing
- SSE endpoint allows real-time log streaming to UI
- Works both in Docker and locally (just need Qdrant running)

---

## Rate Limiting

API rate limits per token (configurable):

| Endpoint Type | Default Limit |
|---------------|---------------|
| Search/query | 60/min |
| Read (list, get) | 120/min |
| Write (create, update) | 30/min |
| Webhooks | 300/min (per repo) |

Headers returned:
```
X-RateLimit-Limit: 60
X-RateLimit-Remaining: 45
X-RateLimit-Reset: 1705312800
```

---

## Webhook Reliability

Outgoing webhooks (to metadata repo) use exponential backoff:

```
Attempt 1: immediate
Attempt 2: 1 min
Attempt 3: 5 min
Attempt 4: 30 min
Attempt 5: 2 hours
```

Failed deliveries tracked in `webhook_deliveries` table:
```sql
webhook_deliveries (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,           -- 'metadata_sync'
  target_url TEXT NOT NULL,
  payload TEXT NOT NULL,
  status TEXT NOT NULL,         -- 'pending', 'success', 'failed'
  attempts INTEGER DEFAULT 0,
  last_attempt_at TEXT,
  next_attempt_at TEXT,
  error TEXT,
  created_at TEXT NOT NULL
)
```

Status endpoint shows pending/failed deliveries.
