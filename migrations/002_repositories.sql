-- Git repository integration tables

-- Connected repositories
CREATE TABLE IF NOT EXISTS repositories (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,                     -- 'github' | 'gitlab'
    owner TEXT NOT NULL,
    repo TEXT NOT NULL,
    branch TEXT NOT NULL,                       -- Single branch to monitor

    -- Webhook
    webhook_id TEXT,
    webhook_secret TEXT,

    -- Auth (encrypted)
    access_token TEXT NOT NULL,

    -- Status
    last_indexed_at TEXT,
    last_commit_sha TEXT,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(project_id, provider, owner, repo, branch)
);

CREATE INDEX IF NOT EXISTS idx_repositories_project ON repositories(project_id);

-- Add foreign key to memories table (already created, adding constraint reference)
-- Note: SQLite doesn't support adding FK constraints after table creation,
-- so we rely on application-level validation for repository_id

-- Raw commit data
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

-- Pull requests
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
