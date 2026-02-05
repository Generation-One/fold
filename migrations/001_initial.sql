-- Fold Complete Schema
-- All tables consolidated into a single initial migration

-- ============================================================================
-- Projects
-- ============================================================================
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    slug TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    description TEXT,

    -- Project paths
    root_path TEXT,
    repo_url TEXT,

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

    -- Webhook loop prevention
    ignored_commit_authors TEXT,                -- JSON array of author patterns to skip

    -- Decay algorithm configuration (ACT-R inspired)
    decay_strength_weight REAL DEFAULT 0.3,     -- Blend weight: 0.0 = pure semantic, 1.0 = pure strength
    decay_half_life_days REAL DEFAULT 30.0,     -- Half-life for exponential decay

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_projects_slug ON projects(slug);

-- ============================================================================
-- Users (created on first OIDC login)
-- ============================================================================
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

-- ============================================================================
-- Project Members (per-project access control)
-- ============================================================================
CREATE TABLE IF NOT EXISTS project_members (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'viewer',         -- 'member' (read/write) | 'viewer' (read-only)
    added_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, project_id)
);

CREATE INDEX IF NOT EXISTS idx_project_members_project ON project_members(project_id);
CREATE INDEX IF NOT EXISTS idx_project_members_user ON project_members(user_id);

-- ============================================================================
-- Sessions (for web UI)
-- ============================================================================
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,                        -- Session cookie value
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);

-- ============================================================================
-- OAuth State (for CSRF protection during OAuth flow)
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
-- API Tokens (for MCP/programmatic access)
-- ============================================================================
CREATE TABLE IF NOT EXISTS api_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,                         -- User-provided description
    token_hash TEXT NOT NULL,                   -- SHA256 of token
    token_prefix TEXT NOT NULL,                 -- First 8 chars for identification
    project_ids TEXT NOT NULL,                  -- JSON array of project IDs
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used TEXT,
    expires_at TEXT,                            -- Optional expiry
    revoked_at TEXT                             -- Revocation timestamp
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_user ON api_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_api_tokens_prefix ON api_tokens(token_prefix);

-- ============================================================================
-- Repositories (file sources - GitHub, GitLab, Google Drive, etc.)
-- ============================================================================
CREATE TABLE IF NOT EXISTS repositories (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,                     -- 'github' | 'gitlab'
    owner TEXT NOT NULL,
    repo TEXT NOT NULL,
    branch TEXT NOT NULL,                       -- Single branch to monitor

    -- Source abstraction
    source_type TEXT,                           -- Same as provider for backwards compat
    source_config TEXT,                         -- JSON for provider-specific config
    notification_type TEXT DEFAULT 'webhook',   -- 'webhook' | 'polling'

    -- Webhook
    webhook_id TEXT,
    webhook_secret TEXT,

    -- Auth (encrypted)
    access_token TEXT NOT NULL,

    -- Status
    last_indexed_at TEXT,
    last_commit_sha TEXT,
    last_sync TEXT,                             -- For polling providers
    sync_cursor TEXT,                           -- Polling state (page token, delta link)

    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(project_id, provider, owner, repo, branch)
);

CREATE INDEX IF NOT EXISTS idx_repositories_project ON repositories(project_id);

-- ============================================================================
-- Memories (metadata - vectors stored in Qdrant)
-- ============================================================================
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

-- ============================================================================
-- Memory links (edges in the knowledge graph)
-- ============================================================================
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

-- ============================================================================
-- Attachments
-- ============================================================================
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

-- ============================================================================
-- Team status
-- ============================================================================
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

-- ============================================================================
-- Git commits (raw commit data)
-- ============================================================================
CREATE TABLE IF NOT EXISTS git_commits (
    id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    sha TEXT NOT NULL,
    message TEXT NOT NULL,
    author_name TEXT,
    author_email TEXT,
    files_changed TEXT,                         -- JSON array of {path, status, additions, deletions}
    insertions INTEGER,
    deletions INTEGER,
    committed_at TEXT NOT NULL,
    indexed_at TEXT NOT NULL DEFAULT (datetime('now')),
    summary_memory_id TEXT,                     -- Links to LLM-generated summary memory
    UNIQUE(repository_id, sha)
);

CREATE INDEX IF NOT EXISTS idx_git_commits_repo ON git_commits(repository_id);
CREATE INDEX IF NOT EXISTS idx_git_commits_sha ON git_commits(sha);
CREATE INDEX IF NOT EXISTS idx_git_commits_date ON git_commits(committed_at);

-- ============================================================================
-- Pull requests
-- ============================================================================
CREATE TABLE IF NOT EXISTS git_pull_requests (
    id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    number INTEGER NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    state TEXT NOT NULL,                        -- 'open' | 'closed' | 'merged'
    author TEXT,
    source_branch TEXT,
    target_branch TEXT,
    created_at TEXT NOT NULL,
    merged_at TEXT,
    indexed_at TEXT NOT NULL DEFAULT (datetime('now')),
    memory_id TEXT,                             -- Links to PR memory
    UNIQUE(repository_id, number)
);

CREATE INDEX IF NOT EXISTS idx_git_prs_repo ON git_pull_requests(repository_id);
CREATE INDEX IF NOT EXISTS idx_git_prs_state ON git_pull_requests(state);

-- ============================================================================
-- Background jobs
-- ============================================================================
CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,                         -- 'index_repo', 'reindex_repo', 'index_history', 'sync_metadata'
    status TEXT NOT NULL DEFAULT 'pending',     -- 'pending', 'running', 'completed', 'failed', 'retry', 'cancelled'
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    repository_id TEXT REFERENCES repositories(id) ON DELETE SET NULL,

    -- Progress
    total_items INTEGER,
    processed_items INTEGER DEFAULT 0,
    failed_items INTEGER DEFAULT 0,

    -- Job queue enhancements
    payload TEXT,                               -- JSON payload for job-specific data
    priority INTEGER DEFAULT 0,                 -- Higher = more urgent
    max_retries INTEGER DEFAULT 3,
    retry_count INTEGER DEFAULT 0,
    locked_at TEXT,                             -- When job was claimed
    locked_by TEXT,                             -- Worker ID that claimed it
    scheduled_at TEXT,                          -- For delayed jobs (NULL = immediate)
    last_error TEXT,                            -- Last error for retries

    -- Timing
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    completed_at TEXT,

    -- Results
    result TEXT,                                -- JSON with summary
    error TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
CREATE INDEX IF NOT EXISTS idx_jobs_project ON jobs(project_id);
CREATE INDEX IF NOT EXISTS idx_jobs_type ON jobs(type, status);
CREATE INDEX IF NOT EXISTS idx_jobs_queue ON jobs(status, priority DESC, scheduled_at, created_at ASC)
    WHERE status IN ('pending', 'retry');
CREATE INDEX IF NOT EXISTS idx_jobs_locked ON jobs(locked_at) WHERE locked_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_jobs_scheduled ON jobs(scheduled_at) WHERE scheduled_at IS NOT NULL;

-- ============================================================================
-- Job logs
-- ============================================================================
CREATE TABLE IF NOT EXISTS job_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    level TEXT NOT NULL,                        -- 'info', 'warn', 'error'
    message TEXT NOT NULL,
    metadata TEXT,                              -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_job_logs_job ON job_logs(job_id);

-- ============================================================================
-- Job execution history
-- ============================================================================
CREATE TABLE IF NOT EXISTS job_executions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    attempt INTEGER NOT NULL,
    worker_id TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    status TEXT NOT NULL,                       -- 'success', 'failed', 'timeout'
    error TEXT,
    duration_ms INTEGER
);

CREATE INDEX IF NOT EXISTS idx_job_executions_job ON job_executions(job_id);

-- ============================================================================
-- AI working sessions
-- ============================================================================
CREATE TABLE IF NOT EXISTS ai_sessions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,

    task TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',      -- 'active', 'paused', 'completed', 'blocked'

    -- Local workspace mapping
    local_root TEXT,
    repository_id TEXT REFERENCES repositories(id) ON DELETE SET NULL,

    -- Session data
    summary TEXT,
    next_steps TEXT,                            -- JSON array

    -- Tracking
    agent_type TEXT,                            -- 'claude-code', 'cursor', etc.
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_ai_sessions_project ON ai_sessions(project_id);
CREATE INDEX IF NOT EXISTS idx_ai_sessions_status ON ai_sessions(status);

-- ============================================================================
-- Session notes
-- ============================================================================
CREATE TABLE IF NOT EXISTS ai_session_notes (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES ai_sessions(id) ON DELETE CASCADE,
    type TEXT NOT NULL,                         -- 'decision', 'blocker', 'question', 'progress', 'finding'
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_session_notes_session ON ai_session_notes(session_id);

-- ============================================================================
-- Workspaces (for local path resolution)
-- ============================================================================
CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    token_id TEXT NOT NULL REFERENCES api_tokens(id) ON DELETE CASCADE,
    local_root TEXT NOT NULL,
    repository_id TEXT REFERENCES repositories(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_workspaces_project ON workspaces(project_id);
CREATE INDEX IF NOT EXISTS idx_workspaces_token ON workspaces(token_id);

-- ============================================================================
-- Webhook delivery tracking
-- ============================================================================
CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,                         -- 'metadata_sync'
    target_url TEXT NOT NULL,
    payload TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',     -- 'pending', 'success', 'failed'
    attempts INTEGER DEFAULT 0,
    last_attempt_at TEXT,
    next_attempt_at TEXT,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_status ON webhook_deliveries(status);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_next ON webhook_deliveries(next_attempt_at);

-- ============================================================================
-- File sync state (for polling providers)
-- ============================================================================
CREATE TABLE IF NOT EXISTS file_sync_state (
    id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    file_hash TEXT,
    file_size INTEGER,
    last_modified TEXT,
    last_checked TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(repository_id, file_path)
);

CREATE INDEX IF NOT EXISTS idx_file_sync_state_repo ON file_sync_state(repository_id);
CREATE INDEX IF NOT EXISTS idx_file_sync_state_path ON file_sync_state(file_path);

-- ============================================================================
-- Provider tokens (OAuth tokens for file source providers)
-- ============================================================================
CREATE TABLE IF NOT EXISTS provider_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider_type TEXT NOT NULL,
    access_token TEXT NOT NULL,
    refresh_token TEXT,
    token_type TEXT DEFAULT 'Bearer',
    expires_at TEXT,
    scopes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(user_id, provider_type)
);

CREATE INDEX IF NOT EXISTS idx_provider_tokens_user ON provider_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_provider_tokens_provider ON provider_tokens(provider_type);

-- ============================================================================
-- Attachment References (for content-addressed storage)
-- ============================================================================
CREATE TABLE IF NOT EXISTS attachment_refs (
    id TEXT PRIMARY KEY,
    memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    hash TEXT NOT NULL,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_attachment_refs_hash ON attachment_refs(hash);
CREATE INDEX IF NOT EXISTS idx_attachment_refs_memory ON attachment_refs(memory_id);

-- Storage path index for memory files
CREATE INDEX IF NOT EXISTS idx_memories_storage_path ON memories(project_id, metadata_repo_file_path);

-- ============================================================================
-- LLM and Embedding Provider Configurations
-- ============================================================================
CREATE TABLE IF NOT EXISTS llm_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,                             -- 'gemini', 'openai', 'anthropic', 'openrouter'
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,            -- Lower number = higher priority

    auth_type TEXT NOT NULL DEFAULT 'api_key',      -- 'api_key' | 'oauth'

    -- API key authentication (encrypted)
    api_key TEXT,

    -- OAuth authentication (encrypted)
    oauth_client_id TEXT,
    oauth_client_secret TEXT,
    oauth_access_token TEXT,
    oauth_refresh_token TEXT,
    oauth_token_expires_at TEXT,
    oauth_scopes TEXT,                              -- JSON array

    -- Provider-specific configuration (JSON)
    config TEXT NOT NULL DEFAULT '{}',              -- { model, endpoint, etc. }

    -- Usage tracking
    request_count INTEGER NOT NULL DEFAULT 0,
    token_count INTEGER NOT NULL DEFAULT 0,         -- Total tokens (input + output)
    error_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,                                -- Last error message
    last_error_at TEXT,                             -- When last error occurred

    -- Metadata
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT,

    UNIQUE(name)
);

CREATE TABLE IF NOT EXISTS embedding_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,                             -- 'gemini', 'openai'
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,            -- Lower number = higher priority

    auth_type TEXT NOT NULL DEFAULT 'api_key',      -- 'api_key' | 'oauth'

    -- API key authentication (encrypted)
    api_key TEXT,

    -- OAuth authentication (encrypted)
    oauth_client_id TEXT,
    oauth_client_secret TEXT,
    oauth_access_token TEXT,
    oauth_refresh_token TEXT,
    oauth_token_expires_at TEXT,
    oauth_scopes TEXT,                              -- JSON array

    -- Provider-specific configuration (JSON)
    config TEXT NOT NULL DEFAULT '{}',              -- { model, dimension, endpoint, etc. }

    -- Usage tracking
    request_count INTEGER NOT NULL DEFAULT 0,
    token_count INTEGER NOT NULL DEFAULT 0,         -- Total tokens used
    error_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,                                -- Last error message
    last_error_at TEXT,                             -- When last error occurred

    -- Metadata
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT,

    UNIQUE(name)
);

-- Indexes for quick lookups
CREATE INDEX IF NOT EXISTS idx_llm_providers_enabled_priority ON llm_providers(enabled, priority);
CREATE INDEX IF NOT EXISTS idx_embedding_providers_enabled_priority ON embedding_providers(enabled, priority);

-- ============================================================================
-- OAuth states for provider authentication (separate from user auth)
-- ============================================================================
CREATE TABLE IF NOT EXISTS provider_oauth_states (
    id TEXT PRIMARY KEY,
    state TEXT UNIQUE NOT NULL,
    provider_type TEXT NOT NULL,                    -- 'llm' | 'embedding'
    provider_name TEXT NOT NULL,                    -- 'gemini', 'openai', etc.
    pkce_verifier TEXT,
    redirect_uri TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_provider_oauth_states_state ON provider_oauth_states(state);
CREATE INDEX IF NOT EXISTS idx_provider_oauth_states_expires ON provider_oauth_states(expires_at);
