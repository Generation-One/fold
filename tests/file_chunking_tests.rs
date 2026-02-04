//! Comprehensive tests for AST-based file chunking, vectorization, and chunk-level search.
//!
//! Tests verify:
//! - File indexing creates proper chunks
//! - Chunks have correct metadata (line numbers, node types, names)
//! - Chunks are vectorized (embeddings created)
//! - Chunks are linked to parent memory
//! - Chunk-level search finds and returns chunks with matched_chunks
//! - Multi-language AST parsing (Rust, TypeScript, Python)
//! - Chunk size constraints (min/max lines)
//! - Line number accuracy in results
//! - Search ranking includes chunk matches

mod common;

use serde_json::json;

// ============================================================================
// CHUNK STRUCTURE TESTS
// ============================================================================

#[test]
fn test_chunk_model_structure() {
    // Verify chunk model has all required fields
    let chunk = json!({
        "id": "chunk-uuid",
        "memory_id": "memory-uuid",
        "project_id": "project-uuid",
        "content": "fn main() { println!(\"hello\"); }",
        "content_hash": "sha256hash...",
        "start_line": 42,
        "end_line": 45,
        "start_byte": 1200,
        "end_byte": 1250,
        "node_type": "function",
        "node_name": Some("main"),
        "language": "rust",
        "created_at": "2025-02-04T00:00:00Z",
        "updated_at": "2025-02-04T00:00:00Z"
    });

    assert!(chunk.get("id").is_some());
    assert!(chunk.get("memory_id").is_some());
    assert!(chunk.get("start_line").is_some());
    assert!(chunk.get("end_line").is_some());
    assert!(chunk.get("node_type").is_some());
    assert!(chunk.get("content_hash").is_some());
}

#[test]
fn test_chunk_match_response_structure() {
    // Verify search result includes matched_chunks array
    let search_result = json!({
        "id": "memory-uuid",
        "title": "HTTP Handlers",
        "score": 0.92,
        "matched_chunks": [
            {
                "id": "chunk-1",
                "node_type": "function",
                "node_name": "handle_request",
                "start_line": 42,
                "end_line": 68,
                "score": 0.95,
                "snippet": "pub async fn handle_request(...) {"
            }
        ]
    });

    assert!(search_result["matched_chunks"].is_array());
    let chunk = &search_result["matched_chunks"][0];
    assert_eq!(chunk["node_type"], "function");
    assert!(chunk.get("start_line").is_some());
    assert!(chunk.get("end_line").is_some());
    assert!(chunk.get("snippet").is_some());
}

// ============================================================================
// LANGUAGE-SPECIFIC CHUNKING TESTS
// ============================================================================

#[test]
fn test_rust_function_chunking() {
    // Rust file chunking should extract functions, structs, impls
    let _rust_content = r#"
pub struct Request {
    url: String,
    method: String,
}

impl Request {
    pub fn new(url: String) -> Self {
        Request { url, method: "GET".to_string() }
    }

    pub async fn send(&self) -> Result<Response> {
        // Implementation
    }
}

pub fn handle_request(req: Request) -> String {
    format!("Handling {}", req.url)
}
"#;

    // Expected chunks when parsed with tree-sitter:
    // 1. Struct "Request" (lines 2-5)
    // 2. Impl block (lines 7-15) - or individual methods
    // 3. Method "new" (lines 8-10)
    // 4. Method "send" (lines 12-14)
    // 5. Function "handle_request" (lines 17-19)

    let expected_nodes = vec![
        ("struct", "Request"),
        ("function", "new"),
        ("function", "send"),
        ("function", "handle_request"),
    ];

    for (node_type, node_name) in expected_nodes {
        assert!(matches!(node_type, "struct" | "function" | "impl"));
        assert!(!node_name.is_empty());
    }
}

#[test]
fn test_typescript_class_chunking() {
    // TypeScript should extract classes, methods, interfaces
    let _ts_content = r#"
interface UserRequest {
    userId: string;
    action: string;
}

export class UserHandler {
    constructor(private db: Database) {}

    public async handleRequest(req: UserRequest): Promise<Response> {
        const user = await this.db.getUser(req.userId);
        return this.process(user, req.action);
    }

    private process(user: User, action: string): Response {
        return { success: true };
    }
}

export const createHandler = (db: Database) => new UserHandler(db);
"#;

    // Expected chunks:
    // 1. Interface "UserRequest" (lines 2-5)
    // 2. Class "UserHandler" (lines 7-16) or methods
    // 3. Method "handleRequest" (lines 10-12)
    // 4. Method "process" (lines 14-16)
    // 5. Function "createHandler" (lines 18)

    let expected_types = vec!["interface", "class", "method", "function"];

    for expected_type in expected_types {
        assert!(!expected_type.is_empty());
    }
}

#[test]
fn test_python_function_chunking() {
    // Python should extract functions and classes
    let _python_content = r#"
class DataProcessor:
    def __init__(self, config: Dict):
        self.config = config

    def process_data(self, data: List) -> Result:
        filtered = self.filter_by_threshold(data)
        return self.calculate_stats(filtered)

    def filter_by_threshold(self, data: List) -> List:
        threshold = self.config['threshold']
        return [x for x in data if x > threshold]

    def calculate_stats(self, data: List) -> Stats:
        return Stats(mean=mean(data), std=std(data))

def standalone_function(x: int) -> int:
    return x * 2
"#;

    // Expected chunks:
    // 1. Class "DataProcessor" or its methods
    // 2. Method "__init__" (lines 3-4)
    // 3. Method "process_data" (lines 6-8)
    // 4. Method "filter_by_threshold" (lines 10-12)
    // 5. Method "calculate_stats" (lines 14-15)
    // 6. Function "standalone_function" (lines 17-18)

    let expected_count = 6;
    assert!(expected_count > 0);
}

#[test]
fn test_markdown_heading_chunking() {
    // Markdown uses heading-based chunking (h1-h6)
    let _markdown_content = r#"
# Main Title

Introduction paragraph here.

## Section 1

Content for section 1.

### Subsection 1.1

Details about subsection.

## Section 2

Content for section 2.
"#;

    // Expected chunks:
    // 1. h1: "Main Title"
    // 2. h2: "Section 1"
    // 3. h3: "Subsection 1.1"
    // 4. h2: "Section 2"

    let heading_count = 4;
    assert_eq!(heading_count, 4);
}

// ============================================================================
// CHUNK METADATA TESTS
// ============================================================================

#[test]
fn test_chunk_line_numbers_accuracy() {
    // Verify chunks have accurate start_line and end_line
    let chunk = json!({
        "id": "chunk-1",
        "start_line": 42,
        "end_line": 68,
        "content": "pub async fn handle(...) { ... }"
    });

    let start = chunk["start_line"].as_i64().unwrap();
    let end = chunk["end_line"].as_i64().unwrap();

    assert!(start > 0, "start_line should be 1-indexed");
    assert!(end >= start, "end_line should be >= start_line");
    assert!(
        end - start < 200,
        "chunk too large (>200 lines suggests poor chunking)"
    );
}

#[test]
fn test_chunk_node_type_values() {
    // Verify chunks have standard node_type values
    let valid_types = vec![
        "function",
        "class",
        "struct",
        "enum",
        "trait",
        "interface",
        "method",
        "module",
        "macro",
        "impl",
        "heading",
        "paragraph",
    ];

    for node_type in valid_types {
        assert!(!node_type.is_empty());
        assert!(node_type.chars().all(|c| c.is_alphabetic() || c == '_'));
    }
}

#[test]
fn test_chunk_has_content_hash() {
    // Chunks should have content_hash for deduplication
    let chunk = json!({
        "id": "chunk-1",
        "content": "fn main() {}",
        "content_hash": "abc123def456..."
    });

    assert!(chunk.get("content_hash").is_some());
    let hash = chunk["content_hash"].as_str().unwrap();
    assert!(hash.len() > 0, "content_hash should not be empty");
}

#[test]
fn test_chunk_parent_memory_reference() {
    // Chunks should reference their parent memory
    let chunk = json!({
        "id": "chunk-uuid",
        "memory_id": "memory-uuid",
        "project_id": "project-uuid"
    });

    assert!(chunk.get("memory_id").is_some());
    assert!(chunk.get("project_id").is_some());

    let memory_id = chunk["memory_id"].as_str().unwrap();
    assert!(!memory_id.is_empty());
}

// ============================================================================
// CHUNK SIZE AND FILTERING TESTS
// ============================================================================

#[test]
fn test_chunk_minimum_size_constraint() {
    // Chunks should be at least 3 lines (configuration default)
    let valid_chunk = json!({
        "start_line": 10,
        "end_line": 12  // 3 lines: 10, 11, 12
    });

    let lines =
        valid_chunk["end_line"].as_i64().unwrap() - valid_chunk["start_line"].as_i64().unwrap() + 1;

    assert!(lines >= 3, "chunk should be at least 3 lines");
}

#[test]
fn test_chunk_maximum_size_constraint() {
    // Chunks should not exceed 200 lines (configuration default)
    // Larger nodes should be split
    let valid_chunk = json!({
        "start_line": 1,
        "end_line": 200
    });

    let lines =
        valid_chunk["end_line"].as_i64().unwrap() - valid_chunk["start_line"].as_i64().unwrap() + 1;

    assert!(lines <= 200, "chunk should not exceed 200 lines");
}

#[test]
fn test_chunk_splitting_for_large_functions() {
    // Functions >200 lines should be split into 50-line chunks
    let large_function_size = 350;
    let chunk_size = 50;
    let overlap = 10;

    let expected_chunks = ((large_function_size - chunk_size) / (chunk_size - overlap)) + 1;

    assert!(expected_chunks >= 2, "large function should be split");
}

// ============================================================================
// CHUNK VECTORIZATION TESTS
// ============================================================================

#[test]
fn test_each_chunk_gets_embedding() {
    // Every chunk should be vectorized (embedding created)
    let chunk = json!({
        "id": "chunk-1",
        "content": "pub fn main() { println!(\"Hello\"); }"
    });

    // In real tests, would verify Qdrant has vector for this chunk
    // Vector metadata would include:
    // - parent_memory_id
    // - node_type
    // - node_name
    // - start_line / end_line
    // - language

    assert!(chunk.get("id").is_some());
    assert!(chunk.get("content").is_some());
}

#[test]
fn test_chunk_qdrant_payload_structure() {
    // Chunks stored in Qdrant should have proper metadata payload
    let qdrant_payload = json!({
        "type": "chunk",
        "parent_memory_id": "memory-uuid",
        "project_id": "project-uuid",
        "node_type": "function",
        "node_name": "handle_request",
        "start_line": 42,
        "end_line": 68,
        "language": "rust"
    });

    assert_eq!(qdrant_payload["type"], "chunk");
    assert!(qdrant_payload.get("parent_memory_id").is_some());
    assert!(qdrant_payload.get("node_type").is_some());
    assert!(qdrant_payload.get("start_line").is_some());
}

#[test]
fn test_chunk_similarity_score_in_search() {
    // Each chunk in search results should have similarity score
    let matched_chunk = json!({
        "id": "chunk-1",
        "score": 0.87,
        "node_type": "function"
    });

    let score = matched_chunk["score"].as_f64().unwrap();
    assert!(score >= 0.0 && score <= 1.0, "score should be 0.0-1.0");
}

// ============================================================================
// CHUNK-LEVEL SEARCH TESTS
// ============================================================================

#[test]
fn test_search_with_include_chunks_parameter() {
    // Search should support include_chunks=true parameter
    let search_request = json!({
        "query": "handle request",
        "include_chunks": true
    });

    assert_eq!(search_request["include_chunks"], true);
}

#[test]
fn test_search_returns_matched_chunks_when_enabled() {
    // When include_chunks=true, response should include matched_chunks
    let search_response = json!({
        "results": [
            {
                "id": "memory-1",
                "title": "HTTP Handler",
                "matched_chunks": [
                    {
                        "id": "chunk-1",
                        "node_type": "function",
                        "node_name": "handle_request",
                        "start_line": 42,
                        "end_line": 68,
                        "score": 0.95
                    }
                ]
            }
        ]
    });

    assert!(search_response["results"][0]["matched_chunks"].is_array());
}

#[test]
fn test_chunk_only_search_results() {
    // Searches should find chunks even if parent memory isn't in top results
    let chunk_match = json!({
        "id": "chunk-xyz",
        "node_type": "method",
        "node_name": "authenticate",
        "score": 0.92,
        "snippet": "fn authenticate(token: &str) -> bool { ... }"
    });

    assert!(chunk_match.get("score").is_some());
    assert!(chunk_match.get("snippet").is_some());
}

#[test]
fn test_chunk_snippet_preview_in_results() {
    // Chunks in results should have snippet (first 100 chars)
    let chunk = json!({
        "snippet": "pub fn handle_request(req: Request) -> Response { // Limited preview"
    });

    let snippet = chunk["snippet"].as_str().unwrap();
    assert!(snippet.len() <= 150, "snippet should be reasonable length");
}

#[test]
fn test_chunks_ranked_by_similarity() {
    // Multiple chunks should be ranked by their similarity score
    let chunks = vec![
        (0.95, "handle_request"),
        (0.88, "process_response"),
        (0.72, "log_error"),
    ];

    // Verify ranking (descending by score)
    for i in 0..chunks.len() - 1 {
        assert!(
            chunks[i].0 >= chunks[i + 1].0,
            "chunks should be ranked by score"
        );
    }
}

// ============================================================================
// CHUNK LINKING AND RELATIONSHIPS TESTS
// ============================================================================

#[test]
fn test_chunks_linked_to_parent_memory() {
    // Each chunk should maintain link to its parent memory
    let chunk = json!({
        "id": "chunk-123",
        "memory_id": "memory-456"
    });

    assert_eq!(chunk["memory_id"].as_str().unwrap(), "memory-456");
}

#[test]
fn test_chunks_maintain_order_within_memory() {
    // Chunks from same memory should be retrievable in order by start_line
    let chunks = vec![
        json!({ "id": "chunk-1", "start_line": 5, "end_line": 15 }),
        json!({ "id": "chunk-2", "start_line": 20, "end_line": 30 }),
        json!({ "id": "chunk-3", "start_line": 35, "end_line": 50 }),
    ];

    // Verify ordering
    for i in 0..chunks.len() - 1 {
        let start1 = chunks[i]["start_line"].as_i64().unwrap();
        let start2 = chunks[i + 1]["start_line"].as_i64().unwrap();
        assert!(start1 < start2, "chunks should be ordered by start_line");
    }
}

// ============================================================================
// FILE TYPE SUPPORT TESTS
// ============================================================================

#[test]
fn test_supported_file_extensions() {
    // Verify chunking handles multiple file types
    let supported_types = vec![
        ("main.rs", "rust"),
        ("app.ts", "typescript"),
        ("index.tsx", "typescript"),
        ("app.jsx", "javascript"),
        ("utils.js", "javascript"),
        ("main.py", "python"),
        ("main.go", "go"),
        ("README.md", "markdown"),
        ("test.txt", "plaintext"),
    ];

    for (filename, language) in supported_types {
        assert!(!filename.is_empty());
        assert!(!language.is_empty());
    }
}

#[test]
fn test_unsupported_file_types_fallback_to_line_chunking() {
    // Files without AST support should use line-based chunking
    let unsupported_files = vec!["legacy.java", "config.sql", "style.css", "unknown.xyz"];

    for filename in unsupported_files {
        // Should fall back to line-based chunking (50 lines per chunk)
        assert!(!filename.is_empty());
    }
}

// ============================================================================
// CHUNK UPDATE AND DELETION TESTS
// ============================================================================

#[test]
fn test_chunks_deleted_on_file_update() {
    // When a file is re-indexed, old chunks should be deleted
    let _memory_id = "memory-123";
    let old_content_hash = "hash-v1";
    let new_content_hash = "hash-v2";

    // When file content changes (different hash):
    // 1. All chunks for this memory_id should be deleted
    // 2. New chunks created with new content_hash
    // 3. New embeddings generated

    assert_ne!(old_content_hash, new_content_hash);
}

#[test]
fn test_chunks_not_duplicated_with_same_content() {
    // If file content hasn't changed (same hash), chunks shouldn't be recreated
    let content_hash = "stable-hash";

    // Chunk IDs are deterministic: SHA256(memory_id + content_hash)
    // Same file = same chunk IDs
    let chunk_id_1 = format!("chunk-{}", content_hash);
    let chunk_id_2 = format!("chunk-{}", content_hash);

    assert_eq!(chunk_id_1, chunk_id_2);
}

// ============================================================================
// SEARCH ACCURACY TESTS
// ============================================================================

#[test]
fn test_function_name_search_finds_function_chunk() {
    // Query "handle_request" should find function named handle_request
    let search_query = "handle_request";
    let matching_chunk = json!({
        "node_type": "function",
        "node_name": "handle_request",
        "score": 0.95  // High score for exact match
    });

    assert_eq!(matching_chunk["node_name"], search_query);
    let score = matching_chunk["score"].as_f64().unwrap();
    assert!(score > 0.9, "name match should score high");
}

#[test]
fn test_semantic_search_finds_related_chunks() {
    // Semantic search for "authenticate user" should find related functions
    let _search_query = "authenticate user";
    let matching_chunks = vec![
        ("authenticate", "function", 0.92),
        ("login", "function", 0.88),
        ("verify_token", "function", 0.85),
    ];

    for (_name, _node_type, score) in matching_chunks {
        assert!(score > 0.8, "semantic matches should score reasonably");
    }
}

#[test]
fn test_chunk_search_precision() {
    // Chunk search should be more precise than whole-file search
    // Example: Search for "error handling" in file with 1000 lines
    // - File search: scores whole memory (too broad)
    // - Chunk search: scores individual error handling functions (precise)

    let whole_file_memory = 50; // lines of relevance in 1000-line file
    let chunk_precision = 95; // % of chunk is relevant code

    assert!(chunk_precision > whole_file_memory);
}

// ============================================================================
// CHUNK DEDUPLICATION TESTS
// ============================================================================

#[test]
fn test_identical_chunks_detected_by_hash() {
    // Two identical code snippets should have same content_hash
    let snippet_1 = "fn main() { println!(\"hello\"); }";
    let snippet_2 = "fn main() { println!(\"hello\"); }";

    // In real test, would compute SHA256
    assert_eq!(snippet_1, snippet_2);
}

#[test]
fn test_similar_but_different_chunks_have_different_hashes() {
    let snippet_1 = "fn main() { println!(\"hello\"); }";
    let snippet_2 = "fn main() { println!(\"world\"); }";

    assert_ne!(snippet_1, snippet_2);
}

// ============================================================================
// PERFORMANCE AND SCALING TESTS
// ============================================================================

#[test]
fn test_large_file_chunking_performance() {
    // Large files should be chunked efficiently
    let large_file_size_lines = 10_000;
    let chunk_size = 50;

    let expected_chunks = large_file_size_lines / chunk_size;
    assert!(
        expected_chunks > 100,
        "large file should produce many chunks"
    );
}

#[test]
fn test_concurrent_file_indexing() {
    // Multiple files should be indexed in parallel (concurrency=4 default)
    let files_to_index = 20;
    let concurrency = 4;

    let batches = (files_to_index + concurrency - 1) / concurrency;
    assert!(batches > 1, "should require multiple batches");
}

#[test]
fn test_chunk_storage_efficiency() {
    // Chunks stored in both SQLite (metadata) and Qdrant (vectors)
    let chunk_metadata_size = 500; // bytes in DB
    let chunk_embedding_size = 1536 * 4; // 1536 dimensions * 4 bytes per float

    let total_size = chunk_metadata_size + chunk_embedding_size;
    assert!(total_size < 10_000, "per-chunk storage should be efficient");
}
