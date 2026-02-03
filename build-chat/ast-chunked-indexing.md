# AST-Based Chunked Indexing

**Status:** Phase 5 Complete - Chunk Search and Linking Working
**Created:** 2026-02-03
**Last Updated:** 2026-02-03

## Overview

Enhance Fold's indexing to store source code chunks alongside LLM-generated summaries. Chunks are parsed using tree-sitter for language-aware AST boundaries (functions, classes, structs) rather than naive line-based splitting.

### Goals

1. **Precise code search** - Find specific functions/classes, not just files
2. **Better holographic links** - Link memories based on chunk-level similarity
3. **Maintain clean memory model** - Chunks are search indexes, not memories

### Non-Goals

- Replacing the existing summary-based indexing (chunks supplement, not replace)
- Full AST analysis (we only need node boundaries for chunking)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Index Flow                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   Source File                                               │
│       │                                                     │
│       ├──► LLM Summary ──► Memory (SQLite) ──► Summary Vec  │
│       │                         │                 (Qdrant)  │
│       │                         │                           │
│       └──► Tree-sitter ──► Chunks ──────────► Chunk Vecs    │
│            (AST parse)      (SQLite)            (Qdrant)    │
│                                │                    │       │
│                                └────────────────────┘       │
│                                    parent_memory_id         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

```
┌─────────────────────────────────────────────────────────────┐
│                       Search Flow                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   Query ──► Embed ──► Search Qdrant (summaries + chunks)    │
│                              │                              │
│                              ▼                              │
│                     Dedupe by parent_memory_id              │
│                              │                              │
│                              ▼                              │
│                     Return memories with matched chunks     │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## Dependencies

### Tree-sitter Crates

```toml
# Cargo.toml additions
tree-sitter = "0.24"

# Language grammars (add as needed)
tree-sitter-rust = "0.23"
tree-sitter-typescript = "0.23"
tree-sitter-javascript = "0.23"
tree-sitter-python = "0.23"
tree-sitter-go = "0.23"
tree-sitter-java = "0.23"
tree-sitter-c = "0.23"
tree-sitter-cpp = "0.23"
```

### Supported Languages (Phase 1)

| Language   | Grammar Crate           | AST Nodes to Extract |
|------------|-------------------------|----------------------|
| Rust       | tree-sitter-rust        | function_item, impl_item, struct_item, enum_item, mod_item |
| TypeScript | tree-sitter-typescript  | function_declaration, class_declaration, interface_declaration, method_definition |
| JavaScript | tree-sitter-javascript  | function_declaration, class_declaration, method_definition |
| Python     | tree-sitter-python      | function_definition, class_definition |
| Go         | tree-sitter-go          | function_declaration, method_declaration, type_declaration |

### Non-Code File Handling (Hybrid Approach)

For non-code files, we use specialised splitters instead of tree-sitter:

| Format | Strategy | Chunk Boundaries |
|--------|----------|------------------|
| **Markdown** | Heading-based | Split at `##`, `###` etc. Keep heading + content together |
| **Plain text** | Paragraph-based | Split on double newlines, with overlap |
| **HTML** | Tag-based | Split on semantic tags (`<section>`, `<article>`, `<div>`) |
| **Unknown** | Line-based fallback | Fixed size (50 lines) with overlap (10 lines) |

```rust
// Chunking strategy selection
fn select_chunker(language: &str) -> ChunkStrategy {
    match language {
        "rust" | "typescript" | "javascript" | "python" | "go" | "java" | "c" | "cpp"
            => ChunkStrategy::TreeSitter,
        "markdown"
            => ChunkStrategy::HeadingBased,
        "html"
            => ChunkStrategy::TagBased,
        ""  // unknown/plain text
            => ChunkStrategy::ParagraphBased,
        _
            => ChunkStrategy::LineBased,
    }
}
```

---

## Data Model

### New Table: `chunks`

```sql
CREATE TABLE chunks (
    id TEXT PRIMARY KEY,
    memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL,

    -- Chunk content
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,

    -- Position in file
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,

    -- AST metadata
    node_type TEXT NOT NULL,        -- "function", "class", "struct", etc.
    node_name TEXT,                 -- Name of the function/class if available
    language TEXT NOT NULL,

    -- Timestamps
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_chunks_memory ON chunks(memory_id);
CREATE INDEX idx_chunks_project ON chunks(project_id);
CREATE INDEX idx_chunks_hash ON chunks(content_hash);
```

### Qdrant Payload Schema

Chunks stored in the same collection as memories, differentiated by `entry_type`:

```json
{
    "id": "chunk-uuid",
    "entry_type": "chunk",
    "parent_memory_id": "memory-uuid",
    "project_slug": "my-project",
    "file_path": "src/auth/middleware.rs",
    "node_type": "function",
    "node_name": "validate_token",
    "start_line": 42,
    "end_line": 67,
    "language": "rust"
}
```

Existing memories have `entry_type: "memory"` (migration needed).

---

## Implementation Phases

### Phase 1: Chunker Service ✅ COMPLETE
- [x] Add tree-sitter dependencies to Cargo.toml
- [x] Create `ChunkerService` in `src/services/chunker.rs`
- [x] Implement `ChunkStrategy` enum and strategy selection
- [x] **Tree-sitter (code):**
  - [x] Implement language detection → parser mapping
  - [x] Implement AST traversal to extract top-level definitions
  - [x] Add unit tests for Rust, TypeScript, Python, Go
- [x] **Markdown chunker:**
  - [x] Implement heading-based splitting
  - [x] Handle frontmatter and code blocks
  - [x] Add unit tests
- [x] **Paragraph chunker (plain text):**
  - [x] Implement double-newline splitting with merge logic
  - [x] Add unit tests
- [x] **Line-based fallback:**
  - [x] Implement with configurable size/overlap
  - [ ] Add brace-balance heuristic (skipped - not needed)
  - [x] Add unit tests

### Phase 2: Data Model ✅ COMPLETE
- [x] Add `chunks` table migration
- [x] Add `entry_type` field to Qdrant payloads (using type: "chunk")
- [x] Create `Chunk` model in `src/models/chunk.rs`
- [x] Add chunk CRUD operations to `src/db/chunks.rs`

### Phase 3: Indexer Integration ✅ COMPLETE
- [x] Modify `IndexerService::index_file()` to also generate chunks
- [x] Embed chunk content via `EmbeddingService`
- [x] Store chunks in SQLite and Qdrant
- [x] Handle chunk updates on file changes (diff by content_hash)
- [x] Delete orphaned chunks when file is removed

### Phase 4: Search Enhancement ✅ COMPLETE
- [x] Modify `MemoryService::search()` to query both memories and chunks
- [x] Implement result deduplication (group chunks by parent memory)
- [x] Return matched chunk context with search results
- [x] Add `include_chunks: bool` parameter to search API

### Phase 5: Linker Enhancement ✅ COMPLETE
- [x] Modify `LinkerService::auto_link()` to use chunk-level similarity
- [x] When chunk A similar to chunk B, link parent memories
- [x] Add link metadata indicating which chunks triggered the link

### Phase 6: Configuration
- [ ] Add `CHUNKING_ENABLED` env var (default: true)
- [ ] Add `CHUNK_MIN_LINES` env var (default: 5)
- [ ] Add config to IndexingConfig struct

---

## Chunker Service Design

```rust
// src/services/chunker.rs

pub enum ChunkStrategy {
    TreeSitter,      // AST-based for code
    HeadingBased,    // For markdown
    ParagraphBased,  // For plain text
    TagBased,        // For HTML
    LineBased,       // Fallback
}

pub struct ChunkerService {
    parsers: HashMap<String, tree_sitter::Parser>,
    config: ChunkerConfig,
}

pub struct ChunkerConfig {
    pub line_chunk_size: usize,     // Default: 50
    pub line_overlap: usize,        // Default: 10
    pub min_chunk_lines: usize,     // Default: 5 (skip tiny chunks)
    pub max_chunk_lines: usize,     // Default: 200 (split huge functions)
}

pub struct CodeChunk {
    pub content: String,
    pub node_type: String,      // "function", "class", "heading", "paragraph", etc.
    pub node_name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
}

impl ChunkerService {
    pub fn new() -> Self;

    /// Parse content and extract semantic chunks based on file type
    pub fn chunk(&self, content: &str, language: &str) -> Vec<CodeChunk>;

    /// Select chunking strategy based on language
    fn select_strategy(&self, language: &str) -> ChunkStrategy;

    // === Tree-sitter (code) ===

    /// Get parser for language, lazily initialised
    fn get_parser(&mut self, language: &str) -> Option<&mut Parser>;

    /// Extract relevant nodes from AST
    fn extract_ast_chunks(
        &self,
        tree: &Tree,
        source: &[u8],
        language: &str
    ) -> Vec<CodeChunk>;

    // === Markdown ===

    /// Split markdown by headings
    fn chunk_markdown(&self, content: &str) -> Vec<CodeChunk>;

    // === Plain text ===

    /// Split text by paragraphs (double newline)
    fn chunk_paragraphs(&self, content: &str) -> Vec<CodeChunk>;

    // === Fallback ===

    /// Line-based chunking with overlap
    fn chunk_lines(&self, content: &str) -> Vec<CodeChunk>;
}
```

### Node Types by Language

```rust
fn interesting_node_types(language: &str) -> &[&str] {
    match language {
        "rust" => &[
            "function_item",
            "impl_item",
            "struct_item",
            "enum_item",
            "trait_item",
            "mod_item",
        ],
        "typescript" | "javascript" => &[
            "function_declaration",
            "class_declaration",
            "interface_declaration",
            "method_definition",
            "arrow_function",  // only top-level/exported
        ],
        "python" => &[
            "function_definition",
            "class_definition",
        ],
        "go" => &[
            "function_declaration",
            "method_declaration",
            "type_declaration",
        ],
        _ => &[],  // Fallback to line-based for unsupported
    }
}
```

---

## Search Result Changes

### Current Response

```json
{
    "memories": [
        {
            "id": "mem-123",
            "title": "Authentication Middleware",
            "content": "Summary of auth middleware...",
            "score": 0.89
        }
    ]
}
```

### Enhanced Response

```json
{
    "memories": [
        {
            "id": "mem-123",
            "title": "Authentication Middleware",
            "content": "Summary of auth middleware...",
            "score": 0.89,
            "matched_chunks": [
                {
                    "id": "chunk-456",
                    "node_type": "function",
                    "node_name": "validate_token",
                    "start_line": 42,
                    "end_line": 67,
                    "score": 0.94,
                    "snippet": "pub async fn validate_token(token: &str) -> Result<Claims> {\n    ..."
                }
            ]
        }
    ]
}
```

---

## Non-Code Chunkers

### Markdown Chunker

Split on heading boundaries, keeping heading + content together:

```rust
fn chunk_markdown(&self, content: &str) -> Vec<CodeChunk> {
    // Regex: ^#{1,6}\s+.+$
    // Split at each heading, create chunk with:
    //   - node_type: "heading" or "h1", "h2", etc.
    //   - node_name: heading text
    //   - content: heading + all content until next heading

    // Handle frontmatter (---) as separate chunk
    // Handle code blocks (```) - don't split inside them
}
```

Example input:
```markdown
# Overview
Some intro text.

## Installation
Steps to install...

## Usage
How to use...
```

Produces 3 chunks:
1. `h1: Overview` + intro
2. `h2: Installation` + steps
3. `h2: Usage` + content

### Paragraph Chunker (Plain Text)

Split on double newlines with configurable overlap:

```rust
fn chunk_paragraphs(&self, content: &str) -> Vec<CodeChunk> {
    // Split on \n\n (or \r\n\r\n)
    // Merge small paragraphs together until min_chunk_lines reached
    // Add overlap from previous chunk if configured
}
```

### Line-Based Fallback

For unknown formats or when other strategies fail:

```rust
fn chunk_lines(&self, content: &str) -> Vec<CodeChunk> {
    // Split every `line_chunk_size` lines (default 50)
    // Include `line_overlap` lines from previous chunk (default 10)
    // Heuristic: try not to split mid-brace (check balance)
}
```

---

## Migration Plan

### Existing Data

1. Add `entry_type = 'memory'` to all existing Qdrant points
2. Re-index projects to generate chunks (can be done incrementally)

### Rollout

1. Deploy with `CHUNKING_ENABLED=false`
2. Run migration to add entry_type to existing points
3. Enable chunking: `CHUNKING_ENABLED=true`
4. Trigger re-index of projects (via API or job)

---

## Testing Strategy

### Unit Tests

- Chunker extracts correct nodes for each language
- Chunk boundaries are accurate (line numbers, byte offsets)
- Fallback works for unknown languages

### Integration Tests

- Index file → chunks created in DB and Qdrant
- Search returns chunks grouped by parent memory
- Delete file → orphaned chunks cleaned up
- Update file → chunks updated correctly

### Live Testing (test/ directory)

Create sample files in `test/sample-files/` for manual testing:

```
test/sample-files/
├── rust-sample.rs       # Functions, structs, impl blocks
├── typescript-sample.ts # Classes, functions, interfaces
├── python-sample.py     # Functions, classes
├── markdown-sample.md   # Multiple headings, code blocks
├── plain-text-sample.txt # Paragraphs of text
└── mixed-project/       # Realistic project structure
    ├── src/
    │   ├── main.rs
    │   └── lib.rs
    ├── README.md
    └── docs/
        └── guide.md
```

**Testing workflow:**
1. Start server with test database
2. Create/use test project pointing to sample-files
3. Trigger index via API
4. Verify chunks in DB: `SELECT * FROM chunks WHERE project_id = ?`
5. Test search: query for function names, headings
6. Verify deduplication and parent memory grouping

**Full access for testing:**
- Can start/stop server via PowerShell scripts
- Can reset database (delete ./data/fold.db)
- Can query Qdrant directly (http://localhost:6334)
- Can use curl/API calls to test endpoints
- Can inspect SQLite directly

**Server management:**
```powershell
# Start server
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "d:/hh/git/g1/fold/srv/start-server.ps1"

# Check status
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "d:/hh/git/g1/fold/srv/check-server.ps1"

# Stop server
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "d:/hh/git/g1/fold/srv/stop-server.ps1"
```

**Database reset:**
```bash
rm ./data/fold.db  # Will be recreated on server start
```

**Qdrant inspection:**
```bash
curl http://localhost:6334/collections
curl http://localhost:6334/collections/fold_<project_slug>/points/scroll
```

---

## Open Questions

1. **Chunk size limits** - Should we split very large functions into sub-chunks?
2. **Nested structures** - Include methods inside classes as separate chunks?
3. **Comments/docs** - Include docstrings in chunk content?
4. **Performance** - Tree-sitter parsing overhead on large repos?

---

---

## Implementation Checklist

**REMINDER:** Check back with this plan regularly during implementation to:
- Verify we're following the design
- Update progress log
- Note any deviations or issues
- Mark completed items

---

## Progress Log

### 2026-02-03
- Initial plan created
- Decided on tree-sitter for AST parsing
- Defined data model and phases
- Added hybrid approach for markdown/plain text
- Added live testing strategy with sample files
- **Phase 1 COMPLETE**: ChunkerService implemented with:
  - Tree-sitter AST chunking for Rust, TS, JS, Python, Go
  - Markdown heading-based chunking
  - Paragraph-based chunking for plain text
  - Line-based fallback with overlap
  - All 4 unit tests passing
- **Sample files created** in test/sample-files/:
  - rust-sample.rs (structs, impls, enums, traits, functions)
  - typescript-sample.ts (classes, interfaces, functions)
  - python-sample.py (classes, functions, decorators)
  - markdown-sample.md (multi-level headings, code blocks)
  - plain-text-sample.txt (paragraphs of meeting notes)
  - mixed-project/ (realistic project structure)

- **Phase 2 COMPLETE**: Data model implemented:
  - chunks table added to schema.sql
  - Chunk model with ChunkCreate and ChunkSearchResult
  - Full CRUD operations in db/chunks.rs
  - All 3 database tests passing

- **Phase 3 COMPLETE**: Indexer integration:
  - Added ChunkerService, EmbeddingService, QdrantService, DbPool to IndexerService
  - Modified index_file() to call process_chunks() after memory creation
  - Chunks embedded via EmbeddingService and stored in Qdrant with parent_memory_id
  - Chunks stored in SQLite via db::insert_chunks()
  - Existing chunks deleted on re-index (handles updates)
  - All builds passing, all tests passing

- **End-to-End Testing SUCCESSFUL**:
  - Server started, database reset, test project created
  - Local repository connected and re-indexed
  - **Results:**
    - 13 files indexed → 13 memories created
    - 84 chunks extracted and stored:
      - 37 markdown chunks (heading-based: h1, h2, h3)
      - 21 rust chunks (tree-sitter: function, struct, impl, enum)
      - 18 typescript chunks (tree-sitter: function, export, class)
      - 8 python chunks (tree-sitter: function, class)
    - 94 Qdrant vectors (memories + chunks)
    - Chunks correctly linked to parent memories

### Next Check-in
- [x] After completing Phase 1 chunker service
- [x] After creating sample test files
- [x] After Phase 2 data model (chunks table)
- [x] After Phase 3 indexer integration
- [x] After first successful index with chunks
- [x] After Phase 4 search enhancement
- [x] After Phase 5 linker enhancement

<!-- Update this section as work progresses -->

IMPORTANT: Afterwards, before saying anything is complete -> you can start the server, reset the db, check the apis / qdrant etc.. see the results in the db, run it, do all of it you can do.. check the chunks etc.. i'll leave you to it.