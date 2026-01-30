-- Enhanced job queue with atomic claiming, priorities, and retry logic

-- Add new columns to jobs table
ALTER TABLE jobs ADD COLUMN payload TEXT;              -- JSON payload for job-specific data
ALTER TABLE jobs ADD COLUMN priority INTEGER DEFAULT 0; -- Higher = more urgent
ALTER TABLE jobs ADD COLUMN max_retries INTEGER DEFAULT 3;
ALTER TABLE jobs ADD COLUMN retry_count INTEGER DEFAULT 0;
ALTER TABLE jobs ADD COLUMN locked_at TEXT;            -- When job was claimed
ALTER TABLE jobs ADD COLUMN locked_by TEXT;            -- Worker ID that claimed it
ALTER TABLE jobs ADD COLUMN scheduled_at TEXT;         -- For delayed jobs (NULL = immediate)
ALTER TABLE jobs ADD COLUMN last_error TEXT;           -- Last error for retries

-- Index for efficient job claiming (pending jobs by priority, then created_at)
CREATE INDEX IF NOT EXISTS idx_jobs_queue ON jobs(status, priority DESC, scheduled_at, created_at ASC)
WHERE status IN ('pending', 'retry');

-- Index for finding stale locked jobs
CREATE INDEX IF NOT EXISTS idx_jobs_locked ON jobs(locked_at) WHERE locked_at IS NOT NULL;

-- Index for scheduled jobs
CREATE INDEX IF NOT EXISTS idx_jobs_scheduled ON jobs(scheduled_at) WHERE scheduled_at IS NOT NULL;

-- Add 'retry' status to track jobs waiting for retry
-- (SQLite doesn't support ALTER CHECK, so we just document the new valid values)
-- Valid status values: 'pending', 'running', 'completed', 'failed', 'retry', 'cancelled'

-- Job execution history for debugging
CREATE TABLE IF NOT EXISTS job_executions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    attempt INTEGER NOT NULL,
    worker_id TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    status TEXT NOT NULL,              -- 'success', 'failed', 'timeout'
    error TEXT,
    duration_ms INTEGER
);

CREATE INDEX IF NOT EXISTS idx_job_executions_job ON job_executions(job_id);
