-- Add local_path column to repositories for local git clones
-- This allows the indexer to work with local files instead of GitHub API

ALTER TABLE repositories ADD COLUMN local_path TEXT;

-- Index for quick lookup by local path
CREATE INDEX IF NOT EXISTS idx_repositories_local_path ON repositories(local_path);
