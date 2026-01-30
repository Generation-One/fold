-- File source provider abstraction
-- Generalizes repositories table to support multiple file source types
-- (GitHub, GitLab, Google Drive, OneDrive, etc.)

-- ============================================================================
-- repositories: Add source_type and generalize fields
-- ============================================================================

-- Add source_type column (same as provider for backwards compatibility)
-- For existing rows, this will match the provider column
ALTER TABLE repositories ADD COLUMN source_type TEXT;

-- Update source_type from provider for existing rows
UPDATE repositories SET source_type = provider WHERE source_type IS NULL;

-- Add source_config for provider-specific configuration as JSON
-- This stores extra config like folder IDs, sync settings, etc.
ALTER TABLE repositories ADD COLUMN source_config TEXT;

-- Add notification_type to track how we receive updates
-- 'webhook' = real-time webhook notifications
-- 'polling' = periodic polling for changes
ALTER TABLE repositories ADD COLUMN notification_type TEXT DEFAULT 'webhook';

-- Add last_sync timestamp for polling providers
ALTER TABLE repositories ADD COLUMN last_sync TEXT;

-- Add sync_cursor for tracking polling state
-- (e.g., page token for Google Drive, delta link for OneDrive)
ALTER TABLE repositories ADD COLUMN sync_cursor TEXT;

-- ============================================================================
-- file_sync_state: Track per-file sync state for polling providers
-- ============================================================================

CREATE TABLE IF NOT EXISTS file_sync_state (
    id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_hash TEXT,
    file_size INTEGER,
    last_modified TEXT,
    last_checked TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
    UNIQUE(repository_id, file_path)
);

CREATE INDEX IF NOT EXISTS idx_file_sync_state_repo ON file_sync_state(repository_id);
CREATE INDEX IF NOT EXISTS idx_file_sync_state_path ON file_sync_state(file_path);

-- ============================================================================
-- provider_tokens: Store OAuth tokens for file source providers
-- ============================================================================

CREATE TABLE IF NOT EXISTS provider_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    provider_type TEXT NOT NULL,
    access_token TEXT NOT NULL,
    refresh_token TEXT,
    token_type TEXT DEFAULT 'Bearer',
    expires_at TEXT,
    scopes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    UNIQUE(user_id, provider_type)
);

CREATE INDEX IF NOT EXISTS idx_provider_tokens_user ON provider_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_provider_tokens_provider ON provider_tokens(provider_type);
