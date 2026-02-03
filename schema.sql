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
    root_path TEXT NOT NULL,
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

    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(project_id, provider, owner, repo, branch)
);

CREATE INDEX IF NOT EXISTS idx_repositories_project ON repositories(project_id);

-- ============================================================================
-- Memories (simplified - content stored in fold/)
-- ============================================================================
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,              -- Content hash (16 chars)
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    repository_id TEXT REFERENCES repositories(id) ON DELETE SET NULL,

    -- Content reference (actual content in fold/)
    content_hash TEXT NOT NULL,       -- Full SHA256
    hash_prefix TEXT NOT NULL,        -- First 2 chars for path

    -- Source tracking
    file_path TEXT,
    language TEXT,
    source TEXT NOT NULL,             -- 'file' | 'manual' | 'generated'

    -- Metadata
    title TEXT,
    author TEXT,
    tags TEXT,                        -- JSON array

    -- Agentic metadata (from A-MEM)
    keywords TEXT,                    -- JSON array - auto-extracted key terms
    context TEXT,                     -- Summary of domain/purpose
    links TEXT,                       -- JSON array - IDs of linked memories

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(project_id, content_hash)
);

CREATE INDEX IF NOT EXISTS idx_memories_project ON memories(project_id);
CREATE INDEX IF NOT EXISTS idx_memories_hash ON memories(content_hash);
CREATE INDEX IF NOT EXISTS idx_memories_file ON memories(repository_id, file_path) WHERE file_path IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_memories_source ON memories(project_id, source);

-- ============================================================================
-- Memory Links (for holographic context reconstruction)
-- ============================================================================
CREATE TABLE IF NOT EXISTS memory_links (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL,          -- 'related' | 'references' | 'depends_on' | 'modifies'
    context TEXT,                     -- Why this link exists
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

    total_items INTEGER,
    processed_items INTEGER DEFAULT 0,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    completed_at TEXT,

    error TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
CREATE INDEX IF NOT EXISTS idx_jobs_project ON jobs(project_id);

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
    expires_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_prefix ON api_tokens(token_prefix);
CREATE INDEX IF NOT EXISTS idx_api_tokens_user ON api_tokens(user_id);

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
    name TEXT NOT NULL,               -- 'gemini' | 'openai' | 'anthropic' | 'openrouter' | 'claudecode'
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    auth_type TEXT NOT NULL DEFAULT 'api_key',  -- 'api_key' | 'oauth'
    api_key TEXT,
    oauth_access_token TEXT,
    oauth_refresh_token TEXT,
    oauth_token_expires_at TEXT,
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
    name TEXT NOT NULL,               -- 'gemini' | 'openai'
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    auth_type TEXT NOT NULL DEFAULT 'api_key',
    api_key TEXT,
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
