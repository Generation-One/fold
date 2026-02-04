# Fold Server Crate Extraction Plan

## Overview

Extract the monolithic `srv` crate (~24,600 lines) into a Cargo workspace with focused, reusable crates. This improves compilation times, clarifies ownership, and enables reuse across projects.

## Target Structure

```
srv/
  Cargo.toml              # Workspace root
  crates/
    fold-models/          # Shared data types
    fold-storage/         # Hash-based file storage
    fold-chunker/         # Tree-sitter code parsing
    fold-qdrant/          # Vector DB wrapper
    fold-embeddings/      # Multi-provider embedding client
    fold-llm/             # Multi-provider LLM client
    fold-git/             # Git operations
    fold-file-source/     # File source provider abstraction
    fold-core/            # Main app (API, services, db, state)
```

## Dependency Graph

```
fold-core
  ├── fold-models
  ├── fold-storage
  ├── fold-chunker
  ├── fold-qdrant
  ├── fold-embeddings
  │     └── fold-models
  ├── fold-llm
  │     └── fold-models
  ├── fold-git
  │     └── fold-storage
  └── fold-file-source
```

---

## Phase 1: Workspace Setup and fold-models

### 1.1 Convert to Workspace

**Files to modify:**
- `Cargo.toml` - Convert to workspace root

**Steps:**
1. Create `crates/` directory
2. Add workspace configuration to root `Cargo.toml`
3. Move existing code to `crates/fold-core/`
4. Verify build still works

### 1.2 Extract fold-models

**Source:** `src/models/` (8 files, ~1,000 lines)

**Contents:**
- `memory.rs` - Memory, MemoryCreate, MemoryUpdate, MemorySearch*
- `chunk.rs` - Chunk, CodeChunk
- `project.rs` - Project, ProjectMember
- `repository.rs` - Repository, RepositoryConfig
- `user.rs` - User, UserRole
- `session.rs` - Session
- `provider.rs` - LlmProvider, EmbeddingProvider
- `team.rs` - Team, Group

**Dependencies:**
- serde, serde_json
- chrono
- uuid

**Steps:**
1. Create `crates/fold-models/Cargo.toml`
2. Copy models to `crates/fold-models/src/`
3. Add `pub use` re-exports in `lib.rs`
4. Update fold-core to depend on fold-models
5. Replace `crate::models::*` imports with `fold_models::*`

---

## Phase 2: Storage Utilities

### 2.1 Extract fold-storage

**Source:** `src/services/fold_storage.rs` (22KB)

**Description:** Hash-based file storage for memories. Stores markdown files with YAML frontmatter in `fold/{a}/{b}/{hash}.md` structure.

**Dependencies:**
- tokio (fs operations)
- sha2 (hashing)
- serde_yaml
- thiserror

**Steps:**
1. Create `crates/fold-storage/`
2. Move `fold_storage.rs` logic
3. Define `FoldStorage` trait and implementation
4. Update fold-core imports

### 2.2 Extract fold-chunker

**Source:** `src/services/chunker.rs` (21KB)

**Description:** Semantic code chunking using tree-sitter AST parsing. Extracts functions, classes, markdown headings as searchable chunks.

**Dependencies:**
- tree-sitter
- tree-sitter-rust
- tree-sitter-typescript
- tree-sitter-python
- tree-sitter-go
- tree-sitter-md
- regex

**Steps:**
1. Create `crates/fold-chunker/`
2. Move chunker logic
3. Define `Chunker` trait with `chunk_file()` method
4. Export `ChunkResult`, `SemanticChunk` types

---

## Phase 3: External Service Wrappers

### 3.1 Extract fold-qdrant

**Source:** `src/services/qdrant.rs` (16KB)

**Description:** Thin wrapper around qdrant-client for vector operations.

**Dependencies:**
- qdrant-client
- tokio

**Public API:**
```rust
pub struct QdrantService { ... }

impl QdrantService {
    pub async fn new(url: &str) -> Result<Self>;
    pub async fn create_collection(&self, name: &str, size: u64) -> Result<()>;
    pub async fn upsert(&self, collection: &str, points: Vec<Point>) -> Result<()>;
    pub async fn search(&self, collection: &str, vector: Vec<f32>, limit: u64) -> Result<Vec<ScoredPoint>>;
    pub async fn delete(&self, collection: &str, ids: Vec<String>) -> Result<()>;
}
```

### 3.2 Extract fold-embeddings

**Source:** `src/services/embeddings.rs` (33KB)

**Description:** Multi-provider embedding service with fallback logic. Supports Gemini, OpenAI-compatible endpoints.

**Dependencies:**
- reqwest
- fold-models (for EmbeddingProvider type)
- tokio
- serde_json

**Public API:**
```rust
pub struct EmbeddingService { ... }

impl EmbeddingService {
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    pub fn dimension(&self) -> usize;
}
```

### 3.3 Extract fold-llm

**Source:** `src/services/llm.rs` (39KB)

**Description:** Multi-provider LLM service. Supports Gemini, Anthropic Claude, OpenRouter, OpenAI.

**Dependencies:**
- reqwest
- fold-models (for LlmProvider type)
- tokio
- serde_json

**Public API:**
```rust
pub struct LlmService { ... }

impl LlmService {
    pub async fn generate(&self, prompt: &str) -> Result<String>;
    pub async fn generate_with_system(&self, system: &str, prompt: &str) -> Result<String>;
    pub async fn generate_json<T: DeserializeOwned>(&self, prompt: &str) -> Result<T>;
}
```

---

## Phase 4: Git and File Sources

### 4.1 Extract fold-git

**Source:** `src/services/git_local.rs` (10KB) + parts of `git.rs` (15KB)

**Description:** Local git operations using git2.

**Dependencies:**
- git2
- fold-storage (for syncing fold/ directory)

**Public API:**
```rust
pub struct GitLocalService { ... }

impl GitLocalService {
    pub fn open(path: &Path) -> Result<Self>;
    pub fn head_commit(&self) -> Result<Commit>;
    pub fn diff(&self, from: &str, to: &str) -> Result<String>;
    pub fn log(&self, limit: usize) -> Result<Vec<Commit>>;
}
```

### 4.2 Extract fold-file-source

**Source:** `src/services/file_source/` (4 files)

**Description:** Trait-based file source abstraction with implementations for GitHub, Google Drive, local filesystem.

**Dependencies:**
- tokio
- async-trait

**Public API:**
```rust
#[async_trait]
pub trait FileSourceProvider: Send + Sync {
    async fn list_files(&self, path: &str) -> Result<Vec<FileInfo>>;
    async fn read_file(&self, path: &str) -> Result<String>;
    async fn watch(&self) -> Result<mpsc::Receiver<ChangeEvent>>;
}
```

---

## Phase 5: Final Cleanup

### 5.1 Update fold-core

After all extractions, fold-core contains:
- `src/api/` - HTTP route handlers
- `src/db/` - SQLite query layer
- `src/middleware/` - Auth middleware
- `src/services/` - Core business logic (memory, indexer, linker, job_worker, git_sync, auth, graph)
- `src/config.rs` - Configuration
- `src/state.rs` - AppState
- `src/error.rs` - Error types

### 5.2 Verify and Test

1. Run `cargo build` from workspace root
2. Run `cargo test` for all crates
3. Start server and verify functionality
4. Update CI/CD if needed

---

## Extraction Order (by dependency)

| Order | Crate | Depends On | Estimated Effort |
|-------|-------|------------|------------------|
| 1 | fold-models | (none) | Small |
| 2 | fold-storage | (none) | Small |
| 3 | fold-chunker | (none) | Medium |
| 4 | fold-qdrant | (none) | Small |
| 5 | fold-embeddings | fold-models | Medium |
| 6 | fold-llm | fold-models | Medium |
| 7 | fold-git | fold-storage | Medium |
| 8 | fold-file-source | (none) | Medium |

---

## Notes

- Each extraction should be a separate commit
- Run tests after each extraction
- Keep backward compatibility during transition
- Consider feature flags for gradual rollout

## Files That Stay in fold-core

These are tightly coupled and should remain together:
- `services/memory.rs` - Orchestrates all services
- `services/indexer.rs` - Complex coordination
- `services/linker.rs` - Deep memory integration
- `services/job_worker.rs` - Depends on everything
- `services/git_sync.rs` - Webhook coordination
- `services/auth.rs` - Could extract later
- `services/graph.rs` - Memory graph operations
- `api/*` - HTTP handlers
- `db/*` - Database queries
- `middleware/*` - Auth middleware
