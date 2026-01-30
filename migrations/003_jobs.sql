-- Background job tracking and AI sessions

-- Background jobs
CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,                         -- 'index_repo', 'reindex_repo', 'index_history', 'sync_metadata'
    status TEXT NOT NULL DEFAULT 'pending',     -- 'pending', 'running', 'completed', 'failed'
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    repository_id TEXT REFERENCES repositories(id) ON DELETE SET NULL,

    -- Progress
    total_items INTEGER,
    processed_items INTEGER DEFAULT 0,
    failed_items INTEGER DEFAULT 0,

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

-- Job logs
CREATE TABLE IF NOT EXISTS job_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    level TEXT NOT NULL,                        -- 'info', 'warn', 'error'
    message TEXT NOT NULL,
    metadata TEXT,                              -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_job_logs_job ON job_logs(job_id);

-- AI working sessions
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

-- Session notes
CREATE TABLE IF NOT EXISTS ai_session_notes (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES ai_sessions(id) ON DELETE CASCADE,
    type TEXT NOT NULL,                         -- 'decision', 'blocker', 'question', 'progress', 'finding'
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_session_notes_session ON ai_session_notes(session_id);

-- Workspace mappings (for local path resolution)
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

-- Webhook delivery tracking
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
