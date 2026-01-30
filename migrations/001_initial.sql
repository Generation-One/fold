-- Fold Initial Schema
-- Core tables for projects, memories, users, and authentication

-- Projects
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    slug TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    description TEXT,

    -- Metadata repo sync config
    metadata_repo_enabled INTEGER DEFAULT 0,
    metadata_repo_mode TEXT,                    -- 'separate' | 'in_repo'
    metadata_repo_provider TEXT,                -- 'github' | 'gitlab'
    metadata_repo_owner TEXT,
    metadata_repo_name TEXT,
    metadata_repo_branch TEXT,
    metadata_repo_token TEXT,                   -- Encrypted
    metadata_repo_source_id TEXT,               -- For 'in_repo' mode
    metadata_repo_path_prefix TEXT DEFAULT '.fold/',

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_projects_slug ON projects(slug);

-- Users (created on first OIDC login)
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,                     -- 'google', 'github', 'corporate', etc.
    subject TEXT NOT NULL,                      -- 'sub' claim from OIDC
    email TEXT,
    display_name TEXT,
    avatar_url TEXT,
    role TEXT NOT NULL DEFAULT 'member',        -- 'admin' | 'member'
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_login TEXT,
    UNIQUE(provider, subject)
);

CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

-- Sessions (for web UI)
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,                        -- Session cookie value
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);

-- API Tokens (for MCP/programmatic access)
CREATE TABLE IF NOT EXISTS api_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,                         -- User-provided description
    token_hash TEXT NOT NULL,                   -- SHA256 of token
    token_prefix TEXT NOT NULL,                 -- First 8 chars for identification
    project_ids TEXT NOT NULL,                  -- JSON array of project IDs
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used TEXT,
    expires_at TEXT                             -- Optional expiry
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_user ON api_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_api_tokens_prefix ON api_tokens(token_prefix);

-- Memories (metadata - vectors stored in Qdrant)
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,                        -- Deterministic for codebase files, UUID for manual
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    repository_id TEXT,                         -- NULL for manual memories

    type TEXT NOT NULL,                         -- 'codebase', 'session', 'spec', 'decision', 'task', 'general', 'commit', 'pr'
    title TEXT,
    content TEXT NOT NULL,                      -- Full content for manual, summary for codebase/commit
    content_hash TEXT,                          -- SHA256 prefix for change detection

    -- Source info (for codebase type)
    file_path TEXT,
    language TEXT,
    git_branch TEXT,
    git_commit_sha TEXT,

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

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_memories_project ON memories(project_id);
CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(project_id, type);
CREATE INDEX IF NOT EXISTS idx_memories_file ON memories(repository_id, file_path);
CREATE INDEX IF NOT EXISTS idx_memories_content_hash ON memories(content_hash);

-- Memory links (edges in the knowledge graph)
CREATE TABLE IF NOT EXISTS memory_links (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,

    source_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL,                    -- 'modifies', 'contains', 'affects', 'implements', etc.

    -- Metadata
    created_by TEXT NOT NULL,                   -- 'system' | 'user' | 'ai'
    confidence REAL,                            -- For AI-suggested links (0.0-1.0)
    context TEXT,                               -- Why this link exists

    -- For code links
    change_type TEXT,                           -- 'added', 'modified', 'deleted'
    additions INTEGER,
    deletions INTEGER,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(source_id, target_id, link_type)
);

CREATE INDEX IF NOT EXISTS idx_links_source ON memory_links(source_id);
CREATE INDEX IF NOT EXISTS idx_links_target ON memory_links(target_id);
CREATE INDEX IF NOT EXISTS idx_links_type ON memory_links(project_id, link_type);

-- Attachments
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

-- Team status
CREATE TABLE IF NOT EXISTS team_status (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    username TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'idle',        -- 'active' | 'idle' | 'away'
    current_task TEXT,
    current_files TEXT,                         -- JSON array
    last_seen TEXT NOT NULL DEFAULT (datetime('now')),
    session_start TEXT,
    UNIQUE(project_id, username)
);

CREATE INDEX IF NOT EXISTS idx_team_status_project ON team_status(project_id);
