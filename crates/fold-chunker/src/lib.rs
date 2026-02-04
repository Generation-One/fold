//! Chunker service for semantic code and text chunking.
//!
//! Uses tree-sitter for AST-based code chunking and custom splitters
//! for markdown and plain text.
//!
//! # Example
//!
//! ```rust
//! use fold_chunker::{ChunkerService, ChunkStrategy};
//!
//! let service = ChunkerService::new();
//! let chunks = service.chunk("fn hello() { }", "rust");
//! ```

use std::collections::HashMap;
use std::sync::Mutex;

use regex::Regex;
use tracing::{debug, warn};

/// Chunking strategy based on file type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkStrategy {
    /// AST-based chunking using tree-sitter
    TreeSitter,
    /// Heading-based chunking for markdown
    HeadingBased,
    /// Paragraph-based chunking for plain text
    ParagraphBased,
    /// Line-based fallback with overlap
    LineBased,
}

/// Configuration for the chunker
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Number of lines per chunk for line-based chunking
    pub line_chunk_size: usize,
    /// Number of overlapping lines between chunks
    pub line_overlap: usize,
    /// Minimum lines for a chunk to be kept
    pub min_chunk_lines: usize,
    /// Maximum lines before splitting large nodes
    pub max_chunk_lines: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            line_chunk_size: 50,
            line_overlap: 10,
            min_chunk_lines: 3,
            max_chunk_lines: 200,
        }
    }
}

/// A semantic chunk of code or text
#[derive(Debug, Clone)]
pub struct CodeChunk {
    /// The actual content of the chunk
    pub content: String,
    /// Type of node: "function", "class", "heading", "paragraph", etc.
    pub node_type: String,
    /// Name of the node if available (function name, heading text, etc.)
    pub node_name: Option<String>,
    /// Starting line number (1-indexed)
    pub start_line: usize,
    /// Ending line number (1-indexed)
    pub end_line: usize,
    /// Starting byte offset
    pub start_byte: usize,
    /// Ending byte offset
    pub end_byte: usize,
}

/// Service for chunking source code and text files
pub struct ChunkerService {
    /// Tree-sitter parsers by language
    parsers: Mutex<HashMap<String, tree_sitter::Parser>>,
    /// Configuration
    config: ChunkerConfig,
    /// Compiled regex for markdown headings
    heading_regex: Regex,
}

impl ChunkerService {
    /// Create a new chunker service with default config
    pub fn new() -> Self {
        Self::with_config(ChunkerConfig::default())
    }

    /// Create a new chunker service with custom config
    pub fn with_config(config: ChunkerConfig) -> Self {
        Self {
            parsers: Mutex::new(HashMap::new()),
            config,
            heading_regex: Regex::new(r"^(#{1,6})\s+(.+)$").unwrap(),
        }
    }

    /// Chunk content based on detected language
    pub fn chunk(&self, content: &str, language: &str) -> Vec<CodeChunk> {
        let strategy = self.select_strategy(language);
        debug!(language = %language, strategy = ?strategy, "Chunking content");

        match strategy {
            ChunkStrategy::TreeSitter => self.chunk_ast(content, language),
            ChunkStrategy::HeadingBased => self.chunk_markdown(content),
            ChunkStrategy::ParagraphBased => self.chunk_paragraphs(content),
            ChunkStrategy::LineBased => self.chunk_lines(content),
        }
    }

    /// Select chunking strategy based on language
    pub fn select_strategy(&self, language: &str) -> ChunkStrategy {
        match language.to_lowercase().as_str() {
            // Systems languages
            "rust" | "c" | "cpp" | "c++" | "cc" | "cxx" | "zig" |
            // JVM languages
            "java" | "kotlin" | "kt" | "scala" |
            // .NET languages
            "csharp" | "cs" | "c#" |
            // Scripting languages
            "python" | "py" | "ruby" | "rb" | "php" | "lua" | "elixir" | "ex" | "exs" |
            "bash" | "sh" | "shell" | "zsh" |
            // Web languages
            "javascript" | "js" | "typescript" | "ts" | "tsx" | "jsx" |
            "html" | "htm" | "css" | "scss" | "sass" |
            // Mobile languages
            "swift" |
            // Other
            "go" | "golang" |
            // Data/config formats
            "json" | "toml" | "yaml" | "yml" => ChunkStrategy::TreeSitter,
            "markdown" | "md" => ChunkStrategy::HeadingBased,
            "" | "text" | "txt" => ChunkStrategy::ParagraphBased,
            _ => ChunkStrategy::LineBased,
        }
    }

    // =========================================================================
    // Tree-sitter AST Chunking
    // =========================================================================

    /// Chunk code using tree-sitter AST
    fn chunk_ast(&self, content: &str, language: &str) -> Vec<CodeChunk> {
        let mut parsers = self.parsers.lock().unwrap();

        // Get or create parser for this language
        if !parsers.contains_key(language) {
            if let Some(parser) = self.create_parser(language) {
                parsers.insert(language.to_string(), parser);
            } else {
                warn!(language = %language, "No tree-sitter grammar, falling back to line-based");
                drop(parsers);
                return self.chunk_lines(content);
            }
        }

        let parser = parsers.get_mut(language).unwrap();
        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => {
                warn!(language = %language, "Tree-sitter parse failed, falling back to line-based");
                drop(parsers);
                return self.chunk_lines(content);
            }
        };

        let interesting_types = self.interesting_node_types(language);
        let mut chunks = Vec::new();
        let source = content.as_bytes();

        self.extract_nodes(&tree.root_node(), source, &interesting_types, &mut chunks);

        // Filter out tiny chunks
        chunks.retain(|c| c.end_line - c.start_line + 1 >= self.config.min_chunk_lines);

        // Split very large chunks
        let mut final_chunks = Vec::new();
        for chunk in chunks {
            if chunk.end_line - chunk.start_line + 1 > self.config.max_chunk_lines {
                final_chunks.extend(self.split_large_chunk(&chunk, content));
            } else {
                final_chunks.push(chunk);
            }
        }

        debug!(language = %language, chunks = final_chunks.len(), "AST chunking complete");
        final_chunks
    }

    /// Create a tree-sitter parser for the given language
    fn create_parser(&self, language: &str) -> Option<tree_sitter::Parser> {
        let mut parser = tree_sitter::Parser::new();

        // Most grammars use the new LanguageFn API (LANGUAGE constant)
        // Some older grammars use the old Language API (language() function)
        let result = match language.to_lowercase().as_str() {
            // Systems languages
            "rust" => parser.set_language(&tree_sitter_rust::LANGUAGE.into()),
            "c" => parser.set_language(&tree_sitter_c::LANGUAGE.into()),
            "cpp" | "c++" | "cc" | "cxx" => parser.set_language(&tree_sitter_cpp::LANGUAGE.into()),
            "zig" => parser.set_language(&tree_sitter_zig::LANGUAGE.into()),
            // JVM languages
            "java" => parser.set_language(&tree_sitter_java::LANGUAGE.into()),
            "kotlin" | "kt" => parser.set_language(&tree_sitter_kotlin_ng::LANGUAGE.into()),
            "scala" => parser.set_language(&tree_sitter_scala::LANGUAGE.into()),
            // .NET languages
            "csharp" | "cs" | "c#" => parser.set_language(&tree_sitter_c_sharp::LANGUAGE.into()),
            // Scripting languages
            "python" | "py" => parser.set_language(&tree_sitter_python::LANGUAGE.into()),
            "ruby" | "rb" => parser.set_language(&tree_sitter_ruby::LANGUAGE.into()),
            "php" => parser.set_language(&tree_sitter_php::LANGUAGE_PHP.into()),
            "lua" => parser.set_language(&tree_sitter_lua::LANGUAGE.into()),
            "elixir" | "ex" | "exs" => parser.set_language(&tree_sitter_elixir::LANGUAGE.into()),
            "bash" | "sh" | "shell" | "zsh" => parser.set_language(&tree_sitter_bash::LANGUAGE.into()),
            // Web languages
            "typescript" | "ts" => parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            "tsx" => parser.set_language(&tree_sitter_typescript::LANGUAGE_TSX.into()),
            "javascript" | "js" => parser.set_language(&tree_sitter_javascript::LANGUAGE.into()),
            "jsx" => parser.set_language(&tree_sitter_javascript::LANGUAGE.into()), // JSX uses same grammar
            "html" | "htm" => parser.set_language(&tree_sitter_html::LANGUAGE.into()),
            "css" | "scss" | "sass" => parser.set_language(&tree_sitter_css::LANGUAGE.into()),
            // Mobile languages
            "swift" => parser.set_language(&tree_sitter_swift::LANGUAGE.into()),
            // Other
            "go" | "golang" => parser.set_language(&tree_sitter_go::LANGUAGE.into()),
            // Data/config formats
            "json" => parser.set_language(&tree_sitter_json::LANGUAGE.into()),
            "toml" => parser.set_language(&tree_sitter_toml_ng::LANGUAGE.into()),
            "yaml" | "yml" => parser.set_language(&tree_sitter_yaml::LANGUAGE.into()),
            _ => return None,
        };

        result.ok()?;
        Some(parser)
    }

    /// Get interesting node types for a language
    fn interesting_node_types(&self, language: &str) -> Vec<&'static str> {
        match language.to_lowercase().as_str() {
            // Systems languages
            "rust" => vec![
                "function_item",
                "impl_item",
                "struct_item",
                "enum_item",
                "trait_item",
                "mod_item",
                "macro_definition",
            ],
            "c" => vec![
                "function_definition",
                "struct_specifier",
                "enum_specifier",
                "type_definition",
                "preproc_function_def",
            ],
            "cpp" | "c++" | "cc" | "cxx" => vec![
                "function_definition",
                "class_specifier",
                "struct_specifier",
                "enum_specifier",
                "namespace_definition",
                "template_declaration",
                "type_definition",
            ],
            "zig" => vec![
                "function_declaration",
                "container_declaration",
                "test_declaration",
            ],
            // JVM languages
            "java" => vec![
                "method_declaration",
                "class_declaration",
                "interface_declaration",
                "enum_declaration",
                "constructor_declaration",
                "annotation_type_declaration",
            ],
            "kotlin" | "kt" => vec![
                "function_declaration",
                "class_declaration",
                "object_declaration",
                "interface_declaration",
                "property_declaration",
            ],
            "scala" => vec![
                "function_definition",
                "class_definition",
                "object_definition",
                "trait_definition",
                "val_definition",
            ],
            // .NET languages
            "csharp" | "cs" | "c#" => vec![
                "method_declaration",
                "class_declaration",
                "struct_declaration",
                "interface_declaration",
                "enum_declaration",
                "namespace_declaration",
                "property_declaration",
                "constructor_declaration",
                "record_declaration",
            ],
            // Scripting languages
            "python" | "py" => vec![
                "function_definition",
                "class_definition",
                "decorated_definition",
            ],
            "ruby" | "rb" => vec![
                "method",
                "singleton_method",
                "class",
                "module",
            ],
            "php" => vec![
                "function_definition",
                "method_declaration",
                "class_declaration",
                "interface_declaration",
                "trait_declaration",
                "namespace_definition",
            ],
            "lua" => vec![
                "function_declaration",
                "function_definition",
                "local_function",
            ],
            "elixir" | "ex" | "exs" => vec![
                "call", // def, defp, defmodule are all calls in Elixir AST
            ],
            "bash" | "sh" | "shell" | "zsh" => vec![
                "function_definition",
            ],
            // Web languages
            "typescript" | "ts" => vec![
                "function_declaration",
                "class_declaration",
                "interface_declaration",
                "type_alias_declaration",
                "method_definition",
                "export_statement",
                "enum_declaration",
            ],
            "tsx" => vec![
                "function_declaration",
                "class_declaration",
                "interface_declaration",
                "type_alias_declaration",
                "method_definition",
                "export_statement",
                "enum_declaration",
                "jsx_element",
            ],
            "javascript" | "js" => vec![
                "function_declaration",
                "class_declaration",
                "method_definition",
                "export_statement",
            ],
            "jsx" => vec![
                "function_declaration",
                "class_declaration",
                "method_definition",
                "export_statement",
                "jsx_element",
            ],
            "html" | "htm" => vec![
                "element",
                "script_element",
                "style_element",
            ],
            "css" | "scss" | "sass" => vec![
                "rule_set",
                "media_statement",
                "keyframes_statement",
                "import_statement",
            ],
            // Mobile languages
            "swift" => vec![
                "function_declaration",
                "class_declaration",
                "struct_declaration",
                "enum_declaration",
                "protocol_declaration",
                "extension_declaration",
            ],
            // Other
            "go" | "golang" => vec![
                "function_declaration",
                "method_declaration",
                "type_declaration",
            ],
            // Data/config formats - use document-level chunking
            "json" => vec!["object", "array"],
            "toml" => vec!["table", "table_array_element"],
            "yaml" | "yml" => vec!["block_mapping", "block_sequence"],
            _ => vec![],
        }
    }

    /// Recursively extract interesting nodes from AST
    fn extract_nodes(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        interesting_types: &[&str],
        chunks: &mut Vec<CodeChunk>,
    ) {
        let node_type = node.kind();

        if interesting_types.contains(&node_type) {
            let content = std::str::from_utf8(&source[node.start_byte()..node.end_byte()])
                .unwrap_or("")
                .to_string();

            let node_name = self.extract_node_name(node, source);

            chunks.push(CodeChunk {
                content,
                node_type: self.normalise_node_type(node_type),
                node_name,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
            });
        } else {
            // Recurse into children
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.extract_nodes(&child, source, interesting_types, chunks);
            }
        }
    }

    /// Extract the name of a node (function name, class name, etc.)
    fn extract_node_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind == "identifier"
                || kind == "name"
                || kind == "type_identifier"
                || kind == "property_identifier"
            {
                return std::str::from_utf8(&source[child.start_byte()..child.end_byte()])
                    .ok()
                    .map(String::from);
            }
        }
        None
    }

    /// Normalise tree-sitter node types to simpler names
    fn normalise_node_type(&self, node_type: &str) -> String {
        match node_type {
            // Functions
            "function_item" | "function_declaration" | "function_definition" |
            "local_function" | "preproc_function_def" => "function",
            // Methods
            "method_definition" | "method_declaration" | "method" | "singleton_method" => "method",
            // Classes
            "class_declaration" | "class_definition" | "class" | "class_specifier" => "class",
            // Structs
            "struct_item" | "struct_declaration" | "struct_specifier" => "struct",
            // Enums
            "enum_item" | "enum_declaration" | "enum_specifier" => "enum",
            // Interfaces/Protocols/Traits
            "trait_item" | "trait_definition" | "trait_declaration" => "trait",
            "interface_declaration" | "protocol_declaration" => "interface",
            // Implementations/Extensions
            "impl_item" | "extension_declaration" => "impl",
            // Modules/Namespaces
            "mod_item" | "module" | "namespace_definition" | "namespace_declaration" => "module",
            // Types
            "type_alias_declaration" | "type_declaration" | "type_definition" => "type",
            // Records (C#)
            "record_declaration" => "record",
            // Objects (Kotlin, Scala)
            "object_declaration" | "object_definition" => "object",
            // Properties
            "property_declaration" | "val_definition" => "property",
            // Constructors
            "constructor_declaration" => "constructor",
            // Annotations
            "annotation_type_declaration" => "annotation",
            // Macros
            "macro_definition" => "macro",
            // Decorated (Python)
            "decorated_definition" => "decorated",
            // Exports
            "export_statement" => "export",
            // Templates (C++)
            "template_declaration" => "template",
            // Container (Zig)
            "container_declaration" => "container",
            // Tests (Zig)
            "test_declaration" => "test",
            // Elixir calls (def, defmodule, etc.)
            "call" => "definition",
            // HTML elements
            "element" | "script_element" | "style_element" => "element",
            // JSX
            "jsx_element" => "component",
            // CSS
            "rule_set" => "rule",
            "media_statement" => "media",
            "keyframes_statement" => "keyframes",
            "import_statement" => "import",
            // Data formats
            "object" | "block_mapping" | "table" => "object",
            "array" | "block_sequence" | "table_array_element" => "array",
            other => other,
        }
        .to_string()
    }

    /// Split a large chunk into smaller pieces
    fn split_large_chunk(&self, chunk: &CodeChunk, _full_content: &str) -> Vec<CodeChunk> {
        let lines: Vec<&str> = chunk.content.lines().collect();
        let num_lines = lines.len();

        if num_lines <= self.config.max_chunk_lines {
            return vec![chunk.clone()];
        }

        let mut result = Vec::new();
        let mut i = 0;
        let mut part = 1;

        while i < num_lines {
            let end = (i + self.config.line_chunk_size).min(num_lines);
            let chunk_lines: Vec<&str> = lines[i..end].to_vec();
            let content = chunk_lines.join("\n");

            // Calculate byte offsets within full content
            let start_line = chunk.start_line + i;
            let end_line = chunk.start_line + end - 1;

            result.push(CodeChunk {
                content,
                node_type: format!("{}_part{}", chunk.node_type, part),
                node_name: chunk
                    .node_name
                    .clone()
                    .map(|n| format!("{} (part {})", n, part)),
                start_line,
                end_line,
                start_byte: 0, // Approximate, recalculate if needed
                end_byte: 0,
            });

            i = end.saturating_sub(self.config.line_overlap);
            if i >= end {
                break;
            }
            part += 1;
        }

        result
    }

    // =========================================================================
    // Markdown Chunking
    // =========================================================================

    /// Chunk markdown by headings
    fn chunk_markdown(&self, content: &str) -> Vec<CodeChunk> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let mut current_heading: Option<(String, String, usize)> = None; // (level, name, start_line)
        let mut current_content = Vec::new();
        let mut in_code_block = false;

        for (i, line) in lines.iter().enumerate() {
            // Track code blocks to avoid splitting inside them
            if line.starts_with("```") {
                in_code_block = !in_code_block;
            }

            if !in_code_block {
                if let Some(caps) = self.heading_regex.captures(line) {
                    // Save previous section
                    if let Some((level, name, start)) = current_heading.take() {
                        if !current_content.is_empty() {
                            let content_str = current_content.join("\n");
                            chunks.push(CodeChunk {
                                content: content_str.clone(),
                                node_type: format!("h{}", level.len()),
                                node_name: Some(name),
                                start_line: start,
                                end_line: i,
                                start_byte: 0,
                                end_byte: 0,
                            });
                        }
                        current_content.clear();
                    }

                    let level = caps.get(1).map(|m| m.as_str()).unwrap_or("#");
                    let name = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
                    current_heading = Some((level.to_string(), name, i + 1));
                }
            }

            current_content.push(*line);
        }

        // Save final section
        if let Some((level, name, start)) = current_heading {
            if !current_content.is_empty() {
                let content_str = current_content.join("\n");
                chunks.push(CodeChunk {
                    content: content_str,
                    node_type: format!("h{}", level.len()),
                    node_name: Some(name),
                    start_line: start,
                    end_line: lines.len(),
                    start_byte: 0,
                    end_byte: 0,
                });
            }
        } else if !current_content.is_empty() {
            // No headings found, treat entire content as one chunk
            chunks.push(CodeChunk {
                content: current_content.join("\n"),
                node_type: "document".to_string(),
                node_name: None,
                start_line: 1,
                end_line: lines.len(),
                start_byte: 0,
                end_byte: 0,
            });
        }

        // Filter tiny chunks
        chunks.retain(|c| c.end_line - c.start_line + 1 >= self.config.min_chunk_lines);

        debug!(chunks = chunks.len(), "Markdown chunking complete");
        chunks
    }

    // =========================================================================
    // Paragraph Chunking
    // =========================================================================

    /// Chunk text by paragraphs
    fn chunk_paragraphs(&self, content: &str) -> Vec<CodeChunk> {
        let mut chunks = Vec::new();

        // Split on double newlines
        let paragraphs: Vec<&str> = content.split("\n\n").collect();

        let mut current_chunk = Vec::new();
        let mut current_lines = 0;
        let mut start_line = 1;
        let mut line_offset = 1;

        for para in paragraphs {
            let para_lines = para.lines().count();

            // If adding this paragraph would exceed max, flush current
            if current_lines + para_lines > self.config.line_chunk_size && !current_chunk.is_empty()
            {
                let content_str = current_chunk.join("\n\n");
                chunks.push(CodeChunk {
                    content: content_str,
                    node_type: "paragraph".to_string(),
                    node_name: None,
                    start_line,
                    end_line: line_offset - 1,
                    start_byte: 0,
                    end_byte: 0,
                });

                current_chunk.clear();
                current_lines = 0;
                start_line = line_offset;
            }

            current_chunk.push(para);
            current_lines += para_lines;
            line_offset += para_lines + 1; // +1 for the blank line
        }

        // Flush remaining
        if !current_chunk.is_empty() {
            let content_str = current_chunk.join("\n\n");
            chunks.push(CodeChunk {
                content: content_str,
                node_type: "paragraph".to_string(),
                node_name: None,
                start_line,
                end_line: line_offset - 1,
                start_byte: 0,
                end_byte: 0,
            });
        }

        // Filter tiny chunks
        chunks.retain(|c| c.end_line - c.start_line + 1 >= self.config.min_chunk_lines);

        debug!(chunks = chunks.len(), "Paragraph chunking complete");
        chunks
    }

    // =========================================================================
    // Line-based Fallback
    // =========================================================================

    /// Chunk by lines with overlap
    fn chunk_lines(&self, content: &str) -> Vec<CodeChunk> {
        let lines: Vec<&str> = content.lines().collect();
        let num_lines = lines.len();

        if num_lines == 0 {
            return Vec::new();
        }

        let mut chunks = Vec::new();
        let mut i = 0;

        while i < num_lines {
            let end = (i + self.config.line_chunk_size).min(num_lines);
            let chunk_lines: Vec<&str> = lines[i..end].to_vec();
            let content_str = chunk_lines.join("\n");

            chunks.push(CodeChunk {
                content: content_str,
                node_type: "lines".to_string(),
                node_name: None,
                start_line: i + 1,
                end_line: end,
                start_byte: 0,
                end_byte: 0,
            });

            // Move forward, but keep overlap
            let next = end.saturating_sub(self.config.line_overlap);
            if next <= i {
                break;
            }
            i = next;

            // Prevent infinite loop on small content
            if i >= end {
                break;
            }
        }

        debug!(chunks = chunks.len(), "Line-based chunking complete");
        chunks
    }
}

impl Default for ChunkerService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_chunking() {
        let service = ChunkerService::new();
        let code = r#"
fn hello() {
    println!("Hello");
}

struct User {
    name: String,
    age: u32,
}

impl User {
    fn new(name: String) -> Self {
        Self { name, age: 0 }
    }
}
"#;

        let chunks = service.chunk(code, "rust");
        assert!(!chunks.is_empty());

        let types: Vec<&str> = chunks.iter().map(|c| c.node_type.as_str()).collect();
        assert!(types.contains(&"function"));
        assert!(types.contains(&"struct"));
        assert!(types.contains(&"impl"));
    }

    #[test]
    fn test_markdown_chunking() {
        let service = ChunkerService::new();
        let md = r#"# Overview

This is the intro.
More text here.

## Installation

Run this command:
```bash
npm install
```

## Usage

Use it like this.
"#;

        let chunks = service.chunk(md, "markdown");
        assert!(!chunks.is_empty());

        let names: Vec<Option<&str>> = chunks.iter().map(|c| c.node_name.as_deref()).collect();
        assert!(names.contains(&Some("Overview")));
        assert!(names.contains(&Some("Installation")));
        assert!(names.contains(&Some("Usage")));
    }

    #[test]
    fn test_paragraph_chunking() {
        let service = ChunkerService::new();
        let text = r#"First paragraph with
multiple lines here.

Second paragraph also
has some content.

Third paragraph.
More content here too.
And more lines."#;

        let chunks = service.chunk(text, "");
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.node_type == "paragraph"));
    }

    #[test]
    fn test_strategy_selection() {
        let service = ChunkerService::new();

        assert_eq!(service.select_strategy("rust"), ChunkStrategy::TreeSitter);
        assert_eq!(
            service.select_strategy("typescript"),
            ChunkStrategy::TreeSitter
        );
        assert_eq!(
            service.select_strategy("markdown"),
            ChunkStrategy::HeadingBased
        );
        assert_eq!(service.select_strategy(""), ChunkStrategy::ParagraphBased);
        assert_eq!(service.select_strategy("unknown"), ChunkStrategy::LineBased);
    }
}
