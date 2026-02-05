-- Migration 003: Content stored externally
-- Content is no longer stored in SQLite - it lives on the filesystem:
-- - Codebase memories: read from source file via file_path
-- - Other memories: stored as .md files in fold/{project}/memories/{id}.md
--
-- Since this is a fresh install (no migration needed), we recreate the memories table
-- with content as nullable and add content_storage column.

-- Drop existing table and recreate with new schema
DROP TABLE IF EXISTS memory_links;
DROP TABLE IF EXISTS attachments;
DROP TABLE IF EXISTS memories;

-- Recreate memories table with content nullable and content_storage column
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,                        -- Deterministic for codebase files, UUID for manual
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    repository_id TEXT,                         -- NULL for manual memories

    type TEXT NOT NULL,                         -- 'codebase', 'session', 'spec', 'decision', 'task', 'general', 'commit', 'pr'
    title TEXT,
    content TEXT,                               -- NULL - content stored externally
    content_hash TEXT,                          -- SHA256 prefix for change detection
    content_storage TEXT NOT NULL DEFAULT 'filesystem', -- 'filesystem' or 'source_file'

    -- Source info (for codebase type)
    file_path TEXT,
    language TEXT,
    git_branch TEXT,
    git_commit_sha TEXT,

    -- Line range for specific code sections
    line_start INTEGER,
    line_end INTEGER,

    -- For commit type
    summary_file_path TEXT,

    -- Metadata repo sync status
    metadata_repo_synced_at TEXT,
    metadata_repo_commit_sha TEXT,
    metadata_repo_file_path TEXT,
    synced_from TEXT,                           -- 'fold' | 'github' | 'gitlab'

    -- Metadata
    author TEXT,
    keywords TEXT,                              -- JSON array
    tags TEXT,                                  -- JSON array
    context TEXT,                               -- Additional context
    metadata TEXT,                              -- Custom metadata as JSON

    -- Task fields
    status TEXT,                                -- Task status
    assignee TEXT,                              -- Task assignee

    -- Usage tracking
    retrieval_count INTEGER DEFAULT 0,
    last_accessed TEXT,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_memories_project ON memories(project_id);
CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(project_id, type);
CREATE INDEX IF NOT EXISTS idx_memories_file ON memories(repository_id, file_path);
CREATE INDEX IF NOT EXISTS idx_memories_content_hash ON memories(content_hash);
CREATE INDEX IF NOT EXISTS idx_memories_content_storage ON memories(content_storage);

-- Recreate memory_links table
CREATE TABLE IF NOT EXISTS memory_links (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL,                    -- 'modifies', 'contains', 'affects', 'implements', etc.
    created_by TEXT NOT NULL DEFAULT 'system',  -- 'system', 'user', 'ai'
    confidence REAL,                            -- For AI-suggested links
    context TEXT,                               -- Explanation of the link
    change_type TEXT,                           -- For code links: 'added', 'modified', 'deleted'
    additions INTEGER,
    deletions INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_id, target_id, link_type)
);

CREATE INDEX IF NOT EXISTS idx_links_project ON memory_links(project_id);
CREATE INDEX IF NOT EXISTS idx_links_source ON memory_links(source_id);
CREATE INDEX IF NOT EXISTS idx_links_target ON memory_links(target_id);
CREATE INDEX IF NOT EXISTS idx_links_type ON memory_links(project_id, link_type);

-- Recreate attachments table
CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY,
    memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    storage_path TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_attachments_memory ON attachments(memory_id);
