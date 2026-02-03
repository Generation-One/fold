-- Update existing projects to enable metadata sync by default
UPDATE projects
SET
  metadata_repo_enabled = 1,
  metadata_repo_mode = 'in_repo',
  metadata_repo_branch = 'main',
  metadata_repo_path_prefix = 'fold/'
WHERE metadata_repo_enabled = 0 OR metadata_repo_enabled IS NULL;
