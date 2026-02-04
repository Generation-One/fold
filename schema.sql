-- Fold Simplified Database Schema
-- Loaded directly at startup (no migrations)
-- Holographic memory system with hash-based storage

-- ============================================================================
-- Projects
-- ============================================================================
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    slug TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    description TEXT,

    -- Root path for project files (where fold/ directory lives)
    root_path TEXT,

    -- Indexing patterns
    index_patterns TEXT,              -- JSON array of glob patterns
    ignore_patterns TEXT,             -- JSON array of glob patterns

    -- Team
    team_members TEXT,                -- JSON array of usernames
    owner TEXT,

    -- Metadata
    metadata TEXT,                    -- JSON object
    repo_url TEXT,

    -- Metadata repo sync config
    metadata_repo_enabled INTEGER NOT NULL DEFAULT 0,
    metadata_repo_mode TEXT,
    metadata_repo_provider TEXT,
    metadata_repo_owner TEXT,
    metadata_repo_name TEXT,
    metadata_repo_branch TEXT,
    metadata_repo_token TEXT,
    metadata_repo_source_id TEXT,
    metadata_repo_path_prefix TEXT,

    -- Algorithm config (per-project)
    ignored_commit_authors TEXT,      -- JSON array
    decay_strength_weight REAL,
    decay_half_life_days REAL,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ============================================================================
-- Repositories
-- ============================================================================
CREATE TABLE IF NOT EXISTS repositories (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,           -- 'github' | 'gitlab' | 'local'

    -- Git info
    owner TEXT,
    repo TEXT,
    branch TEXT NOT NULL DEFAULT 'main',

    -- Source config
    source_type TEXT,
    source_config TEXT,               -- JSON config
    notification_type TEXT,

    -- Webhook info
    webhook_id TEXT,
    webhook_secret TEXT,

    -- Auth
    access_token TEXT,

    -- Local path (for local provider)
    local_path TEXT,

    -- Sync state
    last_sync TEXT,
    last_commit_sha TEXT,
    last_indexed_at TEXT,
    sync_cursor TEXT,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(project_id, provider, owner, repo, branch)
);

CREATE INDEX IF NOT EXISTS idx_repositories_project ON repositories(project_id);

-- ============================================================================
-- Memories (simplified - content stored in fold/)
-- ============================================================================
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    repository_id TEXT REFERENCES repositories(id) ON DELETE SET NULL,

    -- Type and source tracking
    type TEXT,                        -- 'codebase' | 'session' | 'spec' | etc.
    source TEXT,                      -- 'file' | 'manual' | 'generated' | 'agent' | 'git'

    -- Content (stored externally in fold/)
    content TEXT,
    content_hash TEXT,                -- SHA256 for change detection
    content_storage TEXT,             -- 'fold' | 'filesystem' | 'source_file'

    -- Metadata
    title TEXT,
    author TEXT,
    tags TEXT,                        -- JSON array
    keywords TEXT,                    -- JSON array - auto-extracted key terms
    context TEXT,                     -- Summary of domain/purpose

    -- For codebase type
    file_path TEXT,
    summary_file_path TEXT,            -- Path to generated summary file
    language TEXT,
    line_start INTEGER,
    line_end INTEGER,
    git_branch TEXT,
    git_commit_sha TEXT,

    -- Metadata repo sync tracking
    metadata_repo_synced_at TEXT,
    metadata_repo_commit_sha TEXT,
    metadata_repo_file_path TEXT,
    synced_from TEXT,                 -- 'fold' | 'github' | 'gitlab'

    -- For task type
    status TEXT,
    assignee TEXT,

    -- Custom metadata as JSON
    metadata TEXT,

    -- Usage tracking
    retrieval_count INTEGER DEFAULT 0,
    last_accessed TEXT,

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_memories_project ON memories(project_id);
CREATE INDEX IF NOT EXISTS idx_memories_hash ON memories(content_hash);
CREATE INDEX IF NOT EXISTS idx_memories_file ON memories(repository_id, file_path);

-- ============================================================================
-- Chunks (semantic code/text chunks for fine-grained search)
-- ============================================================================
CREATE TABLE IF NOT EXISTS chunks (
    id TEXT PRIMARY KEY,
    memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,

    -- Chunk content
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,

    -- Position in file
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    start_byte INTEGER NOT NULL DEFAULT 0,
    end_byte INTEGER NOT NULL DEFAULT 0,

    -- AST metadata
    node_type TEXT NOT NULL,        -- "function", "class", "struct", "heading", "paragraph", etc.
    node_name TEXT,                 -- Name of the function/class if available
    language TEXT NOT NULL,

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_chunks_memory ON chunks(memory_id);
CREATE INDEX IF NOT EXISTS idx_chunks_project ON chunks(project_id);
CREATE INDEX IF NOT EXISTS idx_chunks_hash ON chunks(content_hash);
CREATE INDEX IF NOT EXISTS idx_chunks_node_type ON chunks(node_type);

-- ============================================================================
-- Memory Links (for holographic context reconstruction)
-- ============================================================================
CREATE TABLE IF NOT EXISTS memory_links (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL,          -- 'related' | 'references' | 'depends_on' | 'modifies'
    created_by TEXT NOT NULL DEFAULT 'system',  -- 'system' | 'user' | 'ai'
    confidence REAL,                  -- AI confidence score (0.0-1.0)
    context TEXT,                     -- Why this link exists
    change_type TEXT,                 -- For code changes: 'added' | 'modified' | 'deleted'
    additions INTEGER,                -- Lines added (for code changes)
    deletions INTEGER,                -- Lines deleted (for code changes)
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(source_id, target_id, link_type)
);

CREATE INDEX IF NOT EXISTS idx_links_source ON memory_links(source_id);
CREATE INDEX IF NOT EXISTS idx_links_target ON memory_links(target_id);
CREATE INDEX IF NOT EXISTS idx_links_project_type ON memory_links(project_id, link_type);

-- ============================================================================
-- Jobs
-- ============================================================================
CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,               -- 'index_repo' | 'sync_repo' | 'reindex'
    status TEXT NOT NULL DEFAULT 'pending',
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    repository_id TEXT REFERENCES repositories(id) ON DELETE SET NULL,

    -- Progress
    total_items INTEGER,
    processed_items INTEGER DEFAULT 0,
    failed_items INTEGER NOT NULL DEFAULT 0,

    -- Job data
    payload TEXT,                     -- JSON payload
    result TEXT,                      -- JSON result

    -- Retry handling
    max_retries INTEGER,
    retry_count INTEGER,
    last_error TEXT,
    error TEXT,

    -- Scheduling
    priority INTEGER NOT NULL DEFAULT 0,
    scheduled_at TEXT,
    locked_at TEXT,
    locked_by TEXT,

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    completed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
CREATE INDEX IF NOT EXISTS idx_jobs_project ON jobs(project_id);
CREATE INDEX IF NOT EXISTS idx_jobs_priority ON jobs(priority, created_at);

-- ============================================================================
-- Job Logs
-- ============================================================================
CREATE TABLE IF NOT EXISTS job_logs (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    level TEXT NOT NULL,              -- 'debug' | 'info' | 'warn' | 'error'
    message TEXT NOT NULL,
    metadata TEXT,                    -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_job_logs_job ON job_logs(job_id);

-- ============================================================================
-- Users (OIDC)
-- ============================================================================
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,          -- 'zitadel', 'google', 'github'
    subject TEXT NOT NULL,           -- 'sub' claim from OIDC
    email TEXT,
    display_name TEXT,
    avatar_url TEXT,
    role TEXT NOT NULL DEFAULT 'member',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_login TEXT,

    UNIQUE(provider, subject)
);

CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);

-- ============================================================================
-- Sessions
-- ============================================================================
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);

-- ============================================================================
-- OAuth States (for OIDC flow)
-- ============================================================================
CREATE TABLE IF NOT EXISTS oauth_states (
    id TEXT PRIMARY KEY,
    state TEXT UNIQUE NOT NULL,
    provider TEXT NOT NULL,
    pkce_verifier TEXT,
    nonce TEXT,
    redirect_uri TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_oauth_states_state ON oauth_states(state);
CREATE INDEX IF NOT EXISTS idx_oauth_states_expires ON oauth_states(expires_at);

-- ============================================================================
-- API Tokens
-- ============================================================================
CREATE TABLE IF NOT EXISTS api_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    project_ids TEXT NOT NULL,        -- JSON array
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used TEXT,
    expires_at TEXT,
    revoked_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_prefix ON api_tokens(token_prefix);
CREATE INDEX IF NOT EXISTS idx_api_tokens_user ON api_tokens(user_id);

-- ============================================================================
-- Groups
-- ============================================================================
CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    is_system INTEGER NOT NULL DEFAULT 0,   -- 1 for protected system groups (admin)
    created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_groups_name ON groups(name);
CREATE INDEX IF NOT EXISTS idx_groups_is_system ON groups(is_system);

-- ============================================================================
-- Group Members (many-to-many: users <-> groups)
-- ============================================================================
CREATE TABLE IF NOT EXISTS group_members (
    group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    added_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (group_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_group_members_user ON group_members(user_id);
CREATE INDEX IF NOT EXISTS idx_group_members_group ON group_members(group_id);

-- ============================================================================
-- Project Members (direct user project access)
-- ============================================================================
CREATE TABLE IF NOT EXISTS project_members (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'viewer',    -- 'member' (read+write) or 'viewer' (read-only)
    added_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, project_id)
);

CREATE INDEX IF NOT EXISTS idx_project_members_user ON project_members(user_id);
CREATE INDEX IF NOT EXISTS idx_project_members_project ON project_members(project_id);

-- ============================================================================
-- Project Group Members (group-based project access)
-- ============================================================================
CREATE TABLE IF NOT EXISTS project_group_members (
    group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'viewer',    -- 'member' (read+write) or 'viewer' (read-only)
    added_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (group_id, project_id)
);

CREATE INDEX IF NOT EXISTS idx_project_group_members_project ON project_group_members(project_id);
CREATE INDEX IF NOT EXISTS idx_project_group_members_group ON project_group_members(group_id);

-- ============================================================================
-- Auth Providers (dynamic OIDC configuration)
-- ============================================================================
CREATE TABLE IF NOT EXISTS auth_providers (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,              -- 'oidc' | 'github'
    display_name TEXT NOT NULL,
    issuer TEXT,
    client_id TEXT NOT NULL,
    client_secret TEXT NOT NULL,
    scopes TEXT,                     -- Space-separated
    icon TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_auth_providers_enabled ON auth_providers(enabled);

-- ============================================================================
-- LLM Providers
-- ============================================================================
CREATE TABLE IF NOT EXISTS llm_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,        -- 'gemini' | 'openai' | 'anthropic' | 'openrouter' | 'claudecode'
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    auth_type TEXT NOT NULL DEFAULT 'api_key',  -- 'api_key' | 'oauth'
    api_key TEXT,
    oauth_client_id TEXT,
    oauth_client_secret TEXT,
    oauth_access_token TEXT,
    oauth_refresh_token TEXT,
    oauth_token_expires_at TEXT,
    oauth_scopes TEXT,
    config TEXT,                      -- JSON config
    request_count INTEGER NOT NULL DEFAULT 0,
    token_count INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    last_error_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_llm_providers_name ON llm_providers(name);
CREATE INDEX IF NOT EXISTS idx_llm_providers_enabled ON llm_providers(enabled);

-- ============================================================================
-- Embedding Providers
-- ============================================================================
CREATE TABLE IF NOT EXISTS embedding_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,        -- 'gemini' | 'openai'
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    auth_type TEXT NOT NULL DEFAULT 'api_key',
    api_key TEXT,
    oauth_client_id TEXT,
    oauth_client_secret TEXT,
    oauth_access_token TEXT,
    oauth_refresh_token TEXT,
    oauth_token_expires_at TEXT,
    oauth_scopes TEXT,
    config TEXT,                      -- JSON config
    request_count INTEGER NOT NULL DEFAULT 0,
    token_count INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    last_error_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_embedding_providers_name ON embedding_providers(name);
CREATE INDEX IF NOT EXISTS idx_embedding_providers_enabled ON embedding_providers(enabled);

-- ============================================================================
-- Algorithm Configuration (per-project search tuning)
-- ============================================================================
CREATE TABLE IF NOT EXISTS algorithm_config (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL UNIQUE REFERENCES projects(id) ON DELETE CASCADE,
    strength_weight REAL NOT NULL DEFAULT 0.3,
    decay_half_life_days REAL NOT NULL DEFAULT 30.0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_algorithm_config_project ON algorithm_config(project_id);
