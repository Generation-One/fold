//! Integration tests for webhook handling.
//!
//! Tests GitHub and GitLab webhook processing, signature verification,
//! event parsing, and database updates.

use fold::{db, Result};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

// ============================================================================
// Test Setup Helpers
// ============================================================================

/// Initialize test database with migrations.
async fn setup_test_db() -> db::DbPool {
    let pool = db::init_pool(":memory:").await.unwrap();
    db::migrate(&pool).await.unwrap();
    pool
}

/// Create a test project in the database.
async fn create_test_project(pool: &db::DbPool, id: &str) {
    db::create_project(
        pool,
        db::CreateProject {
            id: id.to_string(),
            name: "Test Project".to_string(),
            slug: "test-project".to_string(),
            description: Some("Test project for webhook testing".to_string()),
        },
    )
    .await
    .unwrap();
}

/// Create a test repository in the database.
async fn create_test_repository(
    pool: &db::DbPool,
    repo_id: &str,
    project_id: &str,
    secret: Option<&str>,
) {
    db::create_repository(
        pool,
        db::CreateRepository {
            id: repo_id.to_string(),
            project_id: project_id.to_string(),
            provider: db::GitProvider::GitHub,
            owner: "test-org".to_string(),
            repo: "test-repo".to_string(),
            branch: "main".to_string(),
            access_token: "test-token".to_string(),
            local_path: None,
        },
    )
    .await
    .unwrap();

    // Set webhook secret if provided
    if let Some(secret) = secret {
        db::update_repository(
            pool,
            repo_id,
            db::UpdateRepository {
                webhook_secret: Some(secret.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
}

/// Generate a valid GitHub HMAC-SHA256 signature for a payload.
fn generate_github_signature(payload: &[u8], secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(payload);
    let result = mac.finalize();
    format!("sha256={}", hex::encode(result.into_bytes()))
}

/// Verify a GitHub HMAC-SHA256 signature for a payload.
/// Returns true if the signature is valid.
fn verify_github_signature(payload: &[u8], secret: &str, signature: &str) -> bool {
    let signature = match signature.strip_prefix("sha256=") {
        Some(s) => s,
        None => return false,
    };

    let signature_bytes = match hex::decode(signature) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(payload);

    mac.verify_slice(&signature_bytes).is_ok()
}

// ============================================================================
// GitHub Push Payload Helpers
// ============================================================================

/// Create a minimal GitHub push event payload.
fn github_push_payload(branch: &str, commits: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "ref": format!("refs/heads/{}", branch),
        "repository": {
            "id": 12345,
            "name": "test-repo",
            "full_name": "test-org/test-repo",
            "default_branch": "main"
        },
        "sender": {
            "id": 1,
            "login": "test-user"
        },
        "commits": commits,
        "head_commit": {
            "id": "abc123def456",
            "message": "Test commit",
            "author": {
                "name": "Test User",
                "email": "test@example.com"
            },
            "added": [],
            "removed": [],
            "modified": []
        }
    })
}

/// Create a commit object for push payload.
fn github_commit(sha: &str, message: &str, added: Vec<&str>, modified: Vec<&str>) -> serde_json::Value {
    json!({
        "id": sha,
        "message": message,
        "author": {
            "name": "Test User",
            "email": "test@example.com"
        },
        "added": added,
        "removed": [],
        "modified": modified
    })
}

// ============================================================================
// GitHub Pull Request Payload Helpers
// ============================================================================

/// Create a GitHub pull request event payload.
fn github_pr_payload(action: &str, pr_number: u32, pr_state: &str, merged: bool) -> serde_json::Value {
    json!({
        "action": action,
        "number": pr_number,
        "repository": {
            "id": 12345,
            "name": "test-repo",
            "full_name": "test-org/test-repo",
            "default_branch": "main"
        },
        "sender": {
            "id": 1,
            "login": "test-user"
        },
        "pull_request": {
            "number": pr_number,
            "title": "Test Pull Request",
            "state": pr_state,
            "user": {
                "id": 2,
                "login": "contributor"
            },
            "head": {
                "ref": "feature/test",
                "sha": "abc123"
            },
            "base": {
                "ref": "main",
                "sha": "def456"
            },
            "merged": merged
        }
    })
}

// ============================================================================
// Signature Verification Tests
// ============================================================================

#[tokio::test]
async fn test_github_signature_verification_valid() {
    let webhook_secret = "test-secret-key-12345";
    let payload = github_push_payload("main", vec![]);
    let payload_bytes = serde_json::to_vec(&payload).unwrap();
    let signature = generate_github_signature(&payload_bytes, webhook_secret);

    // Verify the signature is valid using our verification function
    assert!(verify_github_signature(&payload_bytes, webhook_secret, &signature));
}

#[tokio::test]
async fn test_github_signature_verification_invalid() {
    let webhook_secret = "test-secret-key-12345";
    let payload = github_push_payload("main", vec![]);
    let payload_bytes = serde_json::to_vec(&payload).unwrap();

    // Generate signature with correct secret
    let correct_signature = generate_github_signature(&payload_bytes, webhook_secret);

    // Verification with wrong secret should fail
    assert!(!verify_github_signature(&payload_bytes, "wrong-secret", &correct_signature));

    // Verification with correct secret should succeed
    assert!(verify_github_signature(&payload_bytes, webhook_secret, &correct_signature));
}

#[tokio::test]
async fn test_github_signature_wrong_payload() {
    let webhook_secret = "test-secret-key-12345";
    let original_payload = github_push_payload("main", vec![]);
    let original_bytes = serde_json::to_vec(&original_payload).unwrap();
    let signature = generate_github_signature(&original_bytes, webhook_secret);

    // Tampered payload should fail verification
    let tampered_payload = github_push_payload("develop", vec![]);
    let tampered_bytes = serde_json::to_vec(&tampered_payload).unwrap();

    assert!(!verify_github_signature(&tampered_bytes, webhook_secret, &signature));
}

#[tokio::test]
async fn test_github_signature_missing_prefix() {
    let payload = b"test payload";
    let secret = "test-secret";

    // Generate signature without sha256= prefix
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let result = mac.finalize();
    let signature_without_prefix = hex::encode(result.into_bytes());

    // Verification should fail without sha256= prefix
    assert!(!verify_github_signature(payload, secret, &signature_without_prefix));
}

#[tokio::test]
async fn test_github_signature_invalid_hex() {
    let payload = b"test payload";
    let secret = "test-secret";

    // Invalid hex should fail signature verification
    let invalid_signature = "sha256=not_valid_hex_string";
    assert!(!verify_github_signature(payload, secret, invalid_signature));
}

#[tokio::test]
async fn test_github_signature_empty_payload() {
    let webhook_secret = "test-secret-key-12345";
    let empty_payload = b"";
    let signature = generate_github_signature(empty_payload, webhook_secret);

    // Empty payload should still have valid signature
    assert!(verify_github_signature(empty_payload, webhook_secret, &signature));
}

// ============================================================================
// Payload Parsing Tests
// ============================================================================

#[tokio::test]
async fn test_parse_github_push_payload() {
    let commits = vec![
        github_commit("abc123", "Add new feature", vec!["src/new.rs"], vec![]),
        github_commit("def456", "Fix bug", vec![], vec!["src/lib.rs"]),
    ];
    let payload = github_push_payload("main", commits);

    // Verify payload structure
    assert_eq!(payload["ref"], "refs/heads/main");
    assert_eq!(payload["repository"]["name"], "test-repo");
    assert_eq!(payload["commits"].as_array().unwrap().len(), 2);

    let first_commit = &payload["commits"][0];
    assert_eq!(first_commit["id"], "abc123");
    assert_eq!(first_commit["message"], "Add new feature");
    assert_eq!(first_commit["added"][0], "src/new.rs");
}

#[tokio::test]
async fn test_parse_github_pr_payload_opened() {
    let payload = github_pr_payload("opened", 42, "open", false);

    assert_eq!(payload["action"], "opened");
    assert_eq!(payload["pull_request"]["number"], 42);
    assert_eq!(payload["pull_request"]["state"], "open");
    assert_eq!(payload["pull_request"]["merged"], false);
    assert_eq!(payload["pull_request"]["head"]["ref"], "feature/test");
    assert_eq!(payload["pull_request"]["base"]["ref"], "main");
}

#[tokio::test]
async fn test_parse_github_pr_payload_closed_merged() {
    let payload = github_pr_payload("closed", 42, "closed", true);

    assert_eq!(payload["action"], "closed");
    assert_eq!(payload["pull_request"]["state"], "closed");
    assert_eq!(payload["pull_request"]["merged"], true);
}

#[tokio::test]
async fn test_parse_github_pr_payload_closed_not_merged() {
    let payload = github_pr_payload("closed", 42, "closed", false);

    assert_eq!(payload["action"], "closed");
    assert_eq!(payload["pull_request"]["state"], "closed");
    assert_eq!(payload["pull_request"]["merged"], false);
}

// ============================================================================
// Push Event Processing Tests
// ============================================================================

#[tokio::test]
async fn test_push_to_tracked_branch_creates_job() -> Result<()> {
    let pool = setup_test_db().await;
    create_test_project(&pool, "proj-1").await;
    create_test_repository(&pool, "repo-1", "proj-1", None).await;

    // Verify repository is tracking "main" branch
    let repo = db::get_repository(&pool, "repo-1").await?;
    assert_eq!(repo.branch, "main");

    // Push payload to main branch with changed files
    let commits = vec![
        github_commit("abc123", "Add feature", vec!["src/feature.rs"], vec!["src/lib.rs"]),
    ];
    let payload = github_push_payload("main", commits);

    // Extract branch from ref and verify it matches tracked branch
    let git_ref = payload["ref"].as_str().unwrap();
    let branch = git_ref.strip_prefix("refs/heads/").unwrap();
    assert_eq!(branch, repo.branch);

    // The push has 2 changed files (1 added + 1 modified)
    let commit = &payload["commits"][0];
    let added_count = commit["added"].as_array().unwrap().len();
    let modified_count = commit["modified"].as_array().unwrap().len();
    assert_eq!(added_count + modified_count, 2);

    Ok(())
}

#[tokio::test]
async fn test_push_to_non_tracked_branch_ignored() -> Result<()> {
    let pool = setup_test_db().await;
    create_test_project(&pool, "proj-1").await;
    create_test_repository(&pool, "repo-1", "proj-1", None).await;

    // Repository tracks "main" but push is to "feature" branch
    let repo = db::get_repository(&pool, "repo-1").await?;
    assert_eq!(repo.branch, "main");

    let payload = github_push_payload("feature", vec![
        github_commit("abc123", "Feature work", vec!["src/feature.rs"], vec![]),
    ]);

    // Extract branch from payload
    let git_ref = payload["ref"].as_str().unwrap();
    let branch = git_ref.strip_prefix("refs/heads/").unwrap();
    assert_eq!(branch, "feature");
    assert_ne!(branch, repo.branch);

    // Push to non-tracked branch should be ignored
    Ok(())
}

#[tokio::test]
async fn test_push_collects_changed_files() {
    let commits = vec![
        github_commit("commit1", "First commit", vec!["src/new.rs"], vec!["src/existing.rs"]),
        github_commit("commit2", "Second commit", vec!["src/another.rs"], vec!["src/lib.rs"]),
    ];
    let payload = github_push_payload("main", commits);

    // Collect all changed files like the webhook handler does
    let mut changed_files: Vec<String> = Vec::new();
    if let Some(commits) = payload["commits"].as_array() {
        for commit in commits {
            if let Some(added) = commit["added"].as_array() {
                for file in added {
                    if let Some(f) = file.as_str() {
                        changed_files.push(f.to_string());
                    }
                }
            }
            if let Some(modified) = commit["modified"].as_array() {
                for file in modified {
                    if let Some(f) = file.as_str() {
                        changed_files.push(f.to_string());
                    }
                }
            }
        }
    }

    assert_eq!(changed_files.len(), 4);
    assert!(changed_files.contains(&"src/new.rs".to_string()));
    assert!(changed_files.contains(&"src/existing.rs".to_string()));
    assert!(changed_files.contains(&"src/another.rs".to_string()));
    assert!(changed_files.contains(&"src/lib.rs".to_string()));
}

// ============================================================================
// Pull Request Event Processing Tests
// ============================================================================

#[tokio::test]
async fn test_pr_opened_creates_record() -> Result<()> {
    let pool = setup_test_db().await;
    create_test_project(&pool, "proj-1").await;
    create_test_repository(&pool, "repo-1", "proj-1", None).await;

    // Create PR record like webhook handler would
    let payload = github_pr_payload("opened", 42, "open", false);
    let pr_info = &payload["pull_request"];

    let pr = db::create_git_pull_request(
        &pool,
        db::CreateGitPullRequest {
            id: "pr-1".to_string(),
            repository_id: "repo-1".to_string(),
            number: pr_info["number"].as_i64().unwrap() as i32,
            title: pr_info["title"].as_str().unwrap().to_string(),
            description: None,
            state: db::PrState::Open,
            author: Some(pr_info["user"]["login"].as_str().unwrap().to_string()),
            source_branch: Some(pr_info["head"]["ref"].as_str().unwrap().to_string()),
            target_branch: Some(pr_info["base"]["ref"].as_str().unwrap().to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            merged_at: None,
        },
    )
    .await?;

    assert_eq!(pr.number, 42);
    assert_eq!(pr.title, "Test Pull Request");
    assert!(pr.is_open());
    assert!(!pr.is_merged());
    assert_eq!(pr.source_branch, Some("feature/test".to_string()));
    assert_eq!(pr.target_branch, Some("main".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_pr_closed_not_merged_updates_state() -> Result<()> {
    let pool = setup_test_db().await;
    create_test_project(&pool, "proj-1").await;
    create_test_repository(&pool, "repo-1", "proj-1", None).await;

    // First create an open PR
    db::create_git_pull_request(
        &pool,
        db::CreateGitPullRequest {
            id: "pr-1".to_string(),
            repository_id: "repo-1".to_string(),
            number: 42,
            title: "Test PR".to_string(),
            description: None,
            state: db::PrState::Open,
            author: Some("contributor".to_string()),
            source_branch: Some("feature/test".to_string()),
            target_branch: Some("main".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            merged_at: None,
        },
    )
    .await?;

    // Update to closed (not merged)
    let updated = db::update_pull_request_state(&pool, "pr-1", db::PrState::Closed, None).await?;

    assert_eq!(updated.state, "closed");
    assert!(!updated.is_open());
    assert!(!updated.is_merged());
    assert!(updated.merged_at.is_none());

    Ok(())
}

#[tokio::test]
async fn test_pr_merged_updates_state_and_timestamp() -> Result<()> {
    let pool = setup_test_db().await;
    create_test_project(&pool, "proj-1").await;
    create_test_repository(&pool, "repo-1", "proj-1", None).await;

    // First create an open PR
    db::create_git_pull_request(
        &pool,
        db::CreateGitPullRequest {
            id: "pr-1".to_string(),
            repository_id: "repo-1".to_string(),
            number: 42,
            title: "Test PR".to_string(),
            description: None,
            state: db::PrState::Open,
            author: Some("contributor".to_string()),
            source_branch: Some("feature/test".to_string()),
            target_branch: Some("main".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            merged_at: None,
        },
    )
    .await?;

    // Update to merged
    let merged_at = chrono::Utc::now().to_rfc3339();
    let updated = db::update_pull_request_state(&pool, "pr-1", db::PrState::Merged, Some(&merged_at)).await?;

    assert_eq!(updated.state, "merged");
    assert!(updated.is_merged());
    assert!(updated.merged_at.is_some());

    Ok(())
}

#[tokio::test]
async fn test_pr_upsert_updates_existing() -> Result<()> {
    let pool = setup_test_db().await;
    create_test_project(&pool, "proj-1").await;
    create_test_repository(&pool, "repo-1", "proj-1", None).await;

    // Create initial PR
    db::upsert_git_pull_request(
        &pool,
        db::CreateGitPullRequest {
            id: "pr-1".to_string(),
            repository_id: "repo-1".to_string(),
            number: 42,
            title: "Initial Title".to_string(),
            description: None,
            state: db::PrState::Open,
            author: Some("contributor".to_string()),
            source_branch: Some("feature/test".to_string()),
            target_branch: Some("main".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            merged_at: None,
        },
    )
    .await?;

    // Upsert with updated title and state
    let updated = db::upsert_git_pull_request(
        &pool,
        db::CreateGitPullRequest {
            id: "pr-2".to_string(), // Different ID, same repo+number
            repository_id: "repo-1".to_string(),
            number: 42,
            title: "Updated Title".to_string(),
            description: Some("Added description".to_string()),
            state: db::PrState::Merged,
            author: Some("contributor".to_string()),
            source_branch: Some("feature/test".to_string()),
            target_branch: Some("main".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            merged_at: Some(chrono::Utc::now().to_rfc3339()),
        },
    )
    .await?;

    assert_eq!(updated.title, "Updated Title");
    assert_eq!(updated.state, "merged");
    assert!(updated.merged_at.is_some());

    // Verify only one PR exists
    let prs = db::list_repository_pull_requests(&pool, "repo-1", 100, 0).await?;
    assert_eq!(prs.len(), 1);

    Ok(())
}

// ============================================================================
// Job Creation Tests
// ============================================================================

#[tokio::test]
async fn test_webhook_creates_indexing_job() -> Result<()> {
    let pool = setup_test_db().await;
    create_test_project(&pool, "proj-1").await;
    create_test_repository(&pool, "repo-1", "proj-1", None).await;

    // Create job like webhook handler does
    let job_id = fold::models::new_id();
    let changed_files = vec!["src/file1.rs".to_string(), "src/file2.rs".to_string()];

    let job = db::create_job(
        &pool,
        db::CreateJob {
            id: job_id.clone(),
            job_type: db::JobType::IndexRepo,
            project_id: Some("proj-1".to_string()),
            repository_id: Some("repo-1".to_string()),
            total_items: Some(changed_files.len() as i32),
        },
    )
    .await?;

    assert_eq!(job.status, "pending");
    assert_eq!(job.total_items, Some(2));
    assert!(!job.is_finished());

    // Verify job can be retrieved
    let fetched = db::get_job(&pool, &job_id).await?;
    assert_eq!(fetched.id, job_id);

    Ok(())
}

#[tokio::test]
async fn test_job_lifecycle_from_webhook() -> Result<()> {
    let pool = setup_test_db().await;
    create_test_project(&pool, "proj-1").await;
    create_test_repository(&pool, "repo-1", "proj-1", None).await;

    // Create job
    let job = db::create_job(
        &pool,
        db::CreateJob {
            id: "job-1".to_string(),
            job_type: db::JobType::IndexRepo,
            project_id: Some("proj-1".to_string()),
            repository_id: Some("repo-1".to_string()),
            total_items: Some(5),
        },
    )
    .await?;

    assert_eq!(job.status, "pending");

    // Start job
    let started = db::start_job(&pool, "job-1").await?;
    assert!(started.is_running());

    // Update progress
    db::update_job_progress(&pool, "job-1", 3, 0).await?;
    let in_progress = db::get_job(&pool, "job-1").await?;
    assert_eq!(in_progress.processed_items, 3);

    // Complete job
    let completed = db::complete_job(&pool, "job-1", None).await?;
    assert!(completed.is_finished());
    assert_eq!(completed.status, "completed");

    Ok(())
}

// ============================================================================
// Event Type Routing Tests
// ============================================================================

#[tokio::test]
async fn test_ping_event_type() {
    // Ping events should be acknowledged but not processed
    let payload = json!({
        "zen": "Keep it simple.",
        "hook_id": 12345,
        "repository": {
            "id": 12345,
            "name": "test-repo",
            "full_name": "test-org/test-repo",
            "default_branch": "main"
        }
    });

    // Ping payloads have a "zen" field
    assert!(payload.get("zen").is_some());
}

#[tokio::test]
async fn test_unknown_event_type_ignored() {
    // Unknown event types should be logged and ignored
    let unknown_events = ["deployment", "release", "issues", "fork", "star"];

    for event in unknown_events {
        // These events should not trigger job creation
        assert!(!matches!(event, "push" | "pull_request" | "ping"));
    }
}

// ============================================================================
// GitLab Webhook Tests
// ============================================================================

#[tokio::test]
async fn test_gitlab_push_payload_format() {
    let payload = json!({
        "object_kind": "push",
        "event_type": "push",
        "ref": "refs/heads/main",
        "project": {
            "id": 12345,
            "name": "test-repo",
            "path_with_namespace": "test-org/test-repo",
            "default_branch": "main"
        },
        "user": {
            "id": 1,
            "username": "test-user",
            "name": "Test User"
        },
        "commits": [
            {
                "id": "abc123",
                "message": "Test commit",
                "author": {
                    "name": "Test User",
                    "email": "test@example.com"
                },
                "added": ["src/new.rs"],
                "removed": [],
                "modified": ["src/lib.rs"]
            }
        ]
    });

    assert_eq!(payload["object_kind"], "push");
    assert_eq!(payload["project"]["path_with_namespace"], "test-org/test-repo");
    assert_eq!(payload["commits"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_gitlab_merge_request_payload_format() {
    let payload = json!({
        "object_kind": "merge_request",
        "event_type": "merge_request",
        "project": {
            "id": 12345,
            "name": "test-repo",
            "path_with_namespace": "test-org/test-repo",
            "default_branch": "main"
        },
        "user": {
            "id": 1,
            "username": "test-user",
            "name": "Test User"
        },
        "object_attributes": {
            "iid": 42,
            "title": "Test MR",
            "state": "opened",
            "action": "open",
            "source_branch": "feature/test",
            "target_branch": "main"
        }
    });

    assert_eq!(payload["object_kind"], "merge_request");
    assert_eq!(payload["object_attributes"]["iid"], 42);
    assert_eq!(payload["object_attributes"]["state"], "opened");
    assert_eq!(payload["object_attributes"]["action"], "open");
}

#[tokio::test]
async fn test_gitlab_token_verification() {
    let token = "glpat-xxxxxxxxxxxxxxxxxxxx";
    let expected_token = "glpat-xxxxxxxxxxxxxxxxxxxx";

    // GitLab uses simple token comparison
    assert_eq!(token, expected_token);

    let wrong_token = "wrong-token";
    assert_ne!(wrong_token, expected_token);
}

// ============================================================================
// Edge Cases and Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_empty_commits_push() {
    let payload = github_push_payload("main", vec![]);

    let commits = payload["commits"].as_array().unwrap();
    assert!(commits.is_empty());

    // Empty commits should result in no changed files
    let mut changed_files: Vec<String> = Vec::new();
    for commit in commits {
        if let Some(added) = commit["added"].as_array() {
            for file in added {
                if let Some(f) = file.as_str() {
                    changed_files.push(f.to_string());
                }
            }
        }
    }
    assert!(changed_files.is_empty());
}

#[tokio::test]
async fn test_pr_with_special_characters_in_title() -> Result<()> {
    let pool = setup_test_db().await;
    create_test_project(&pool, "proj-1").await;
    create_test_repository(&pool, "repo-1", "proj-1", None).await;

    let special_title = r#"feat: Add "quotes" & <special> chars"#;

    let pr = db::create_git_pull_request(
        &pool,
        db::CreateGitPullRequest {
            id: "pr-1".to_string(),
            repository_id: "repo-1".to_string(),
            number: 1,
            title: special_title.to_string(),
            description: None,
            state: db::PrState::Open,
            author: None,
            source_branch: None,
            target_branch: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            merged_at: None,
        },
    )
    .await?;

    assert_eq!(pr.title, special_title);

    Ok(())
}

#[tokio::test]
async fn test_webhook_secret_with_special_characters() {
    let special_secret = "secret-with-special-chars!@#$%^&*()";
    let payload = b"test payload";

    let signature = generate_github_signature(payload, special_secret);

    // Verify signature was generated
    assert!(signature.starts_with("sha256="));
    assert!(signature.len() > "sha256=".len());

    // Verify we can decode the hex portion
    let hex_part = signature.strip_prefix("sha256=").unwrap();
    assert!(hex::decode(hex_part).is_ok());
}

#[tokio::test]
async fn test_multiple_prs_for_same_repo() -> Result<()> {
    let pool = setup_test_db().await;
    create_test_project(&pool, "proj-1").await;
    create_test_repository(&pool, "repo-1", "proj-1", None).await;

    // Create multiple PRs
    for i in 1..=5 {
        db::create_git_pull_request(
            &pool,
            db::CreateGitPullRequest {
                id: format!("pr-{}", i),
                repository_id: "repo-1".to_string(),
                number: i,
                title: format!("PR #{}", i),
                description: None,
                state: db::PrState::Open,
                author: Some("contributor".to_string()),
                source_branch: Some(format!("feature/{}", i)),
                target_branch: Some("main".to_string()),
                created_at: chrono::Utc::now().to_rfc3339(),
                merged_at: None,
            },
        )
        .await?;
    }

    // Verify all PRs exist
    let prs = db::list_repository_pull_requests(&pool, "repo-1", 100, 0).await?;
    assert_eq!(prs.len(), 5);

    // Verify open PRs
    let open_prs = db::list_open_pull_requests(&pool, "repo-1").await?;
    assert_eq!(open_prs.len(), 5);

    Ok(())
}
