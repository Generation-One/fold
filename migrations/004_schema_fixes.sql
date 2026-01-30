-- Schema fixes to align migrations with Rust model definitions
-- Addresses missing columns discovered during MCP endpoint testing

-- ============================================================================
-- api_tokens: Add revoked_at column
-- Required by: src/middleware/token_auth.rs (ApiTokenRow struct, line 50)
-- ============================================================================
ALTER TABLE api_tokens ADD COLUMN revoked_at TEXT;

-- ============================================================================
-- memories: Add missing columns from src/models/memory.rs (Memory struct)
-- ============================================================================

-- Context field for additional memory context
-- Required by: src/models/memory.rs:251
ALTER TABLE memories ADD COLUMN context TEXT;

-- Line range for codebase memories (specific code sections)
-- Required by: src/models/memory.rs:256-257
ALTER TABLE memories ADD COLUMN line_start INTEGER;
ALTER TABLE memories ADD COLUMN line_end INTEGER;

-- Task status and assignee fields
-- Required by: src/models/memory.rs:260-261
ALTER TABLE memories ADD COLUMN status TEXT;
ALTER TABLE memories ADD COLUMN assignee TEXT;

-- Custom metadata as JSON
-- Required by: src/models/memory.rs:264
ALTER TABLE memories ADD COLUMN metadata TEXT;

-- Usage tracking fields
-- Required by: src/models/memory.rs:271-272
ALTER TABLE memories ADD COLUMN retrieval_count INTEGER DEFAULT 0;
ALTER TABLE memories ADD COLUMN last_accessed TEXT;
