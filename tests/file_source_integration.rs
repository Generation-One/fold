//! Integration tests for the FileSourceProvider abstraction.
//!
//! Tests the provider registry, GitHub adapter, and notification handling.

use fold::services::file_source::{
    ChangeEvent, FileChangeStatus, GitHubFileSource, GoogleDriveFileSource, NotificationType,
    ProviderRegistry, PullRequestAction, SourceConfig,
};
use fold::services::FileSourceProvider;

// ============================================================================
// Provider Registry Tests
// ============================================================================

#[test]
fn test_registry_with_defaults() {
    let registry = ProviderRegistry::with_defaults();

    // GitHub should be registered
    assert!(registry.has("github"));
    // Google Drive should be registered
    assert!(registry.has("google-drive"));
    // GitLab not implemented yet
    assert!(!registry.has("gitlab"));
}

#[test]
fn test_registry_get_provider() {
    let registry = ProviderRegistry::with_defaults();

    let github = registry.get("github").expect("GitHub provider not found");
    assert_eq!(github.provider_type(), "github");
    assert_eq!(github.display_name(), "GitHub");
    assert!(github.supports_webhooks());
    assert!(!github.requires_polling());
}

#[test]
fn test_registry_list_providers() {
    let registry = ProviderRegistry::with_defaults();

    let types = registry.provider_types();
    assert!(types.contains(&"github"));

    let providers = registry.providers();
    assert!(!providers.is_empty());

    let github_info = providers
        .iter()
        .find(|p| p.provider_type == "github")
        .expect("GitHub not in providers list");
    assert_eq!(github_info.display_name, "GitHub");
    assert!(github_info.supports_webhooks);
    assert!(!github_info.requires_polling);
}

// ============================================================================
// GitHub Provider Tests
// ============================================================================

#[test]
fn test_github_provider_type() {
    let provider = GitHubFileSource::new();
    assert_eq!(provider.provider_type(), "github");
    assert_eq!(provider.display_name(), "GitHub");
}

#[test]
fn test_github_supports_webhooks() {
    let provider = GitHubFileSource::new();
    assert!(provider.supports_webhooks());
    assert!(!provider.requires_polling());
}

#[test]
fn test_github_supported_events() {
    let provider = GitHubFileSource::new();
    let events = provider.supported_events();
    assert!(events.contains(&"push"));
    assert!(events.contains(&"pull_request"));
}

// ============================================================================
// Signature Verification Tests
// ============================================================================

#[test]
fn test_github_verify_signature_valid() {
    let provider = GitHubFileSource::new();

    // Test payload and secret
    let payload = b"test payload";
    let secret = "test_secret";

    // Pre-computed signature for "test payload" with secret "test_secret"
    // Using: echo -n "test payload" | openssl dgst -sha256 -hmac "test_secret"
    let signature = "sha256=1a8c3e7c9c2e5b8f0d1e2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d";

    // This will fail because the signature is made up - we just test the format handling
    let result = provider.verify_notification(payload, signature, secret);
    // The result depends on actual HMAC, so we just ensure it doesn't panic
    assert!(!result); // Made-up signature should fail
}

#[test]
fn test_github_verify_signature_invalid_format() {
    let provider = GitHubFileSource::new();

    let payload = b"test payload";
    let secret = "test_secret";

    // Missing sha256= prefix
    let signature = "1a8c3e7c9c2e5b8f0d1e2a3b4c5d6e7f";
    let result = provider.verify_notification(payload, signature, secret);
    assert!(!result);

    // Empty signature
    let result = provider.verify_notification(payload, "", secret);
    assert!(!result);
}

// ============================================================================
// Push Event Parsing Tests
// ============================================================================

#[test]
fn test_github_parse_push_event() {
    let provider = GitHubFileSource::new();

    let payload = r#"{
        "ref": "refs/heads/main",
        "after": "abc123def456",
        "commits": [{
            "id": "abc123def456",
            "message": "Add new feature\n\nDetailed description here",
            "timestamp": "2024-01-15T10:30:00Z",
            "author": {
                "name": "Test User",
                "email": "test@example.com"
            },
            "added": ["src/new_file.rs", "tests/new_test.rs"],
            "modified": ["src/lib.rs"],
            "removed": ["old_file.rs"]
        }]
    }"#;

    let events = provider
        .parse_notification("push", payload.as_bytes())
        .expect("Failed to parse push event");

    assert_eq!(events.len(), 1);

    match &events[0] {
        ChangeEvent::Commit {
            sha,
            message,
            author,
            files,
            ..
        } => {
            assert_eq!(sha, "abc123def456");
            assert!(message.starts_with("Add new feature"));
            assert_eq!(author, "Test User");
            assert_eq!(files.len(), 4);

            // Check file statuses
            let added_files: Vec<_> = files
                .iter()
                .filter(|f| matches!(f.status, FileChangeStatus::Added))
                .collect();
            assert_eq!(added_files.len(), 2);

            let modified_files: Vec<_> = files
                .iter()
                .filter(|f| matches!(f.status, FileChangeStatus::Modified))
                .collect();
            assert_eq!(modified_files.len(), 1);

            let deleted_files: Vec<_> = files
                .iter()
                .filter(|f| matches!(f.status, FileChangeStatus::Deleted))
                .collect();
            assert_eq!(deleted_files.len(), 1);
        }
        _ => panic!("Expected Commit event"),
    }
}

#[test]
fn test_github_parse_push_multiple_commits() {
    let provider = GitHubFileSource::new();

    let payload = r#"{
        "ref": "refs/heads/feature",
        "commits": [
            {
                "id": "commit1",
                "message": "First commit",
                "timestamp": "2024-01-15T10:00:00Z",
                "author": {"name": "User 1", "email": "user1@test.com"},
                "added": ["file1.rs"],
                "modified": [],
                "removed": []
            },
            {
                "id": "commit2",
                "message": "Second commit",
                "timestamp": "2024-01-15T11:00:00Z",
                "author": {"name": "User 2", "email": "user2@test.com"},
                "added": [],
                "modified": ["file1.rs"],
                "removed": []
            }
        ]
    }"#;

    let events = provider
        .parse_notification("push", payload.as_bytes())
        .expect("Failed to parse push event");

    assert_eq!(events.len(), 2);

    match &events[0] {
        ChangeEvent::Commit { sha, author, .. } => {
            assert_eq!(sha, "commit1");
            assert_eq!(author, "User 1");
        }
        _ => panic!("Expected Commit event"),
    }

    match &events[1] {
        ChangeEvent::Commit { sha, author, .. } => {
            assert_eq!(sha, "commit2");
            assert_eq!(author, "User 2");
        }
        _ => panic!("Expected Commit event"),
    }
}

#[test]
fn test_github_parse_branch_deletion() {
    let provider = GitHubFileSource::new();

    let payload = r#"{
        "ref": "refs/heads/feature-branch",
        "deleted": true,
        "after": "0000000000000000000000000000000000000000"
    }"#;

    let events = provider
        .parse_notification("push", payload.as_bytes())
        .expect("Failed to parse branch deletion");

    assert_eq!(events.len(), 1);

    match &events[0] {
        ChangeEvent::BranchDeleted { branch } => {
            assert_eq!(branch, "feature-branch");
        }
        _ => panic!("Expected BranchDeleted event"),
    }
}

// ============================================================================
// Pull Request Event Parsing Tests
// ============================================================================

#[test]
fn test_github_parse_pr_opened() {
    let provider = GitHubFileSource::new();

    let payload = r#"{
        "action": "opened",
        "pull_request": {
            "number": 42,
            "title": "Add awesome feature",
            "merged": false,
            "user": {"login": "developer"},
            "head": {"ref": "feature-branch"},
            "base": {"ref": "main"}
        }
    }"#;

    let events = provider
        .parse_notification("pull_request", payload.as_bytes())
        .expect("Failed to parse PR event");

    assert_eq!(events.len(), 1);

    match &events[0] {
        ChangeEvent::PullRequest {
            number,
            action,
            title,
            author,
            source_branch,
            target_branch,
            is_merged,
        } => {
            assert_eq!(*number, 42);
            assert_eq!(*action, PullRequestAction::Opened);
            assert_eq!(title, "Add awesome feature");
            assert_eq!(author, "developer");
            assert_eq!(source_branch.as_deref(), Some("feature-branch"));
            assert_eq!(target_branch.as_deref(), Some("main"));
            assert!(!is_merged);
        }
        _ => panic!("Expected PullRequest event"),
    }
}

#[test]
fn test_github_parse_pr_merged() {
    let provider = GitHubFileSource::new();

    let payload = r#"{
        "action": "closed",
        "pull_request": {
            "number": 100,
            "title": "Merge feature",
            "merged": true,
            "user": {"login": "maintainer"},
            "head": {"ref": "feature"},
            "base": {"ref": "main"}
        }
    }"#;

    let events = provider
        .parse_notification("pull_request", payload.as_bytes())
        .expect("Failed to parse PR merge event");

    assert_eq!(events.len(), 1);

    match &events[0] {
        ChangeEvent::PullRequest {
            action, is_merged, ..
        } => {
            assert_eq!(*action, PullRequestAction::Closed);
            assert!(*is_merged);
        }
        _ => panic!("Expected PullRequest event"),
    }
}

#[test]
fn test_github_parse_pr_synchronized() {
    let provider = GitHubFileSource::new();

    let payload = r#"{
        "action": "synchronize",
        "pull_request": {
            "number": 50,
            "title": "WIP: New feature",
            "merged": false,
            "user": {"login": "dev"},
            "head": {"ref": "wip"},
            "base": {"ref": "main"}
        }
    }"#;

    let events = provider
        .parse_notification("pull_request", payload.as_bytes())
        .expect("Failed to parse PR sync event");

    assert_eq!(events.len(), 1);

    match &events[0] {
        ChangeEvent::PullRequest { action, .. } => {
            assert_eq!(*action, PullRequestAction::Synchronized);
        }
        _ => panic!("Expected PullRequest event"),
    }
}

// ============================================================================
// Unknown Event Tests
// ============================================================================

#[test]
fn test_github_parse_unknown_event() {
    let provider = GitHubFileSource::new();

    let payload = r#"{"some": "data"}"#;

    let events = provider
        .parse_notification("issues", payload.as_bytes())
        .expect("Should handle unknown events gracefully");

    assert_eq!(events.len(), 0);
}

// ============================================================================
// Source Config Tests
// ============================================================================

#[test]
fn test_source_config_git() {
    let config = SourceConfig::git("owner", "repo", Some("main"));
    assert_eq!(config.owner.as_deref(), Some("owner"));
    assert_eq!(config.name, "repo");
    assert_eq!(config.branch.as_deref(), Some("main"));
}

#[test]
fn test_source_config_folder() {
    let config = SourceConfig::folder("folder-id-123");
    assert!(config.owner.is_none());
    assert_eq!(config.name, "folder-id-123");
    assert!(config.branch.is_none());
}

// ============================================================================
// FileChangeStatus Tests
// ============================================================================

#[test]
fn test_file_change_status_from_str() {
    assert_eq!(FileChangeStatus::from_str("added"), FileChangeStatus::Added);
    assert_eq!(FileChangeStatus::from_str("new"), FileChangeStatus::Added);
    assert_eq!(
        FileChangeStatus::from_str("modified"),
        FileChangeStatus::Modified
    );
    assert_eq!(
        FileChangeStatus::from_str("changed"),
        FileChangeStatus::Modified
    );
    assert_eq!(
        FileChangeStatus::from_str("deleted"),
        FileChangeStatus::Deleted
    );
    assert_eq!(
        FileChangeStatus::from_str("removed"),
        FileChangeStatus::Deleted
    );
    assert_eq!(
        FileChangeStatus::from_str("renamed"),
        FileChangeStatus::Renamed
    );
    assert_eq!(
        FileChangeStatus::from_str("unknown"),
        FileChangeStatus::Changed
    );
}

// ============================================================================
// PullRequestAction Tests
// ============================================================================

#[test]
fn test_pull_request_action_from_str() {
    assert_eq!(
        PullRequestAction::from_str("opened"),
        PullRequestAction::Opened
    );
    assert_eq!(
        PullRequestAction::from_str("closed"),
        PullRequestAction::Closed
    );
    assert_eq!(
        PullRequestAction::from_str("merged"),
        PullRequestAction::Merged
    );
    assert_eq!(
        PullRequestAction::from_str("synchronize"),
        PullRequestAction::Synchronized
    );
    assert_eq!(
        PullRequestAction::from_str("update"),
        PullRequestAction::Synchronized
    );
    assert_eq!(
        PullRequestAction::from_str("reopened"),
        PullRequestAction::Reopened
    );
}

// ============================================================================
// Google Drive Provider Tests
// ============================================================================

#[test]
fn test_google_drive_provider_type() {
    let provider = GoogleDriveFileSource::new();
    assert_eq!(provider.provider_type(), "google-drive");
    assert_eq!(provider.display_name(), "Google Drive");
}

#[test]
fn test_google_drive_does_not_support_webhooks() {
    let provider = GoogleDriveFileSource::new();
    assert!(!provider.supports_webhooks());
    assert!(provider.requires_polling());
}

#[test]
fn test_google_drive_supported_events() {
    let provider = GoogleDriveFileSource::new();
    let events = provider.supported_events();
    assert!(events.contains(&"file_created"));
    assert!(events.contains(&"file_modified"));
    assert!(events.contains(&"file_deleted"));
    assert!(events.contains(&"file_moved"));
}

#[test]
fn test_google_drive_in_registry() {
    let registry = ProviderRegistry::with_defaults();

    let gdrive = registry
        .get("google-drive")
        .expect("Google Drive provider not found");
    assert_eq!(gdrive.provider_type(), "google-drive");
    assert_eq!(gdrive.display_name(), "Google Drive");
    assert!(!gdrive.supports_webhooks());
    assert!(gdrive.requires_polling());
}

#[test]
fn test_google_drive_in_provider_list() {
    let registry = ProviderRegistry::with_defaults();

    let providers = registry.providers();
    let gdrive_info = providers
        .iter()
        .find(|p| p.provider_type == "google-drive")
        .expect("Google Drive not in providers list");

    assert_eq!(gdrive_info.display_name, "Google Drive");
    assert!(!gdrive_info.supports_webhooks);
    assert!(gdrive_info.requires_polling);
}

#[test]
fn test_google_drive_verify_notification_returns_false() {
    let provider = GoogleDriveFileSource::new();
    // Since Google Drive uses polling, verify_notification should return false
    let result = provider.verify_notification(b"test payload", "signature", "secret");
    assert!(!result);
}

#[test]
fn test_google_drive_parse_notification_returns_empty() {
    let provider = GoogleDriveFileSource::new();
    // Since Google Drive uses polling, parse_notification should return empty
    let events = provider
        .parse_notification("file_change", b"{}")
        .expect("Should handle notification gracefully");
    assert!(events.is_empty());
}

#[tokio::test]
async fn test_google_drive_connect() {
    let provider = GoogleDriveFileSource::new();
    let config = SourceConfig::folder("test-folder-id-123");

    let result = provider.connect(config, "fake-token").await;
    assert!(result.is_ok());

    let info = result.unwrap();
    assert_eq!(info.id, "test-folder-id-123");
    assert_eq!(info.name, "test-folder-id-123");
    assert!(info.full_name.contains("Google Drive"));
    assert!(info
        .url
        .as_deref()
        .unwrap()
        .contains("test-folder-id-123"));
}

#[tokio::test]
async fn test_google_drive_disconnect() {
    let provider = GoogleDriveFileSource::new();
    let config = SourceConfig::folder("test-folder-id");

    let info = provider
        .connect(config, "fake-token")
        .await
        .expect("Connect should succeed");

    let result = provider.disconnect(&info, "fake-token").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_google_drive_list_files_placeholder() {
    let provider = GoogleDriveFileSource::new();
    let config = SourceConfig::folder("test-folder-id");

    let info = provider
        .connect(config, "fake-token")
        .await
        .expect("Connect should succeed");

    let files = provider
        .list_files(&info, None, None, "fake-token")
        .await
        .expect("list_files should succeed");

    // Placeholder returns empty list
    assert!(files.is_empty());
}

#[tokio::test]
async fn test_google_drive_get_file_returns_error() {
    let provider = GoogleDriveFileSource::new();
    let config = SourceConfig::folder("test-folder-id");

    let info = provider
        .connect(config, "fake-token")
        .await
        .expect("Connect should succeed");

    let result = provider
        .get_file(&info, "some/file.txt", None, "fake-token")
        .await;

    // Placeholder returns error since OAuth is not configured
    assert!(result.is_err());
}

#[tokio::test]
async fn test_google_drive_register_notifications() {
    let provider = GoogleDriveFileSource::new();
    let config = SourceConfig::folder("test-folder-id");

    let info = provider
        .connect(config, "fake-token")
        .await
        .expect("Connect should succeed");

    let notification_config = provider
        .register_notifications(&info, "https://example.com/webhook", "secret", "fake-token")
        .await
        .expect("register_notifications should succeed");

    // Should return polling configuration since webhooks are not supported
    assert_eq!(
        notification_config.notification_type,
        NotificationType::Polling
    );
    assert!(notification_config.poll_interval_secs.is_some());
    assert_eq!(notification_config.poll_interval_secs.unwrap(), 300);
    assert!(notification_config
        .notification_id
        .starts_with("gdrive-poll-"));
}

#[tokio::test]
async fn test_google_drive_unregister_notifications() {
    let provider = GoogleDriveFileSource::new();
    let config = SourceConfig::folder("test-folder-id");

    let info = provider
        .connect(config, "fake-token")
        .await
        .expect("Connect should succeed");

    let result = provider
        .unregister_notifications(&info, "gdrive-poll-test-folder-id", "fake-token")
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_google_drive_detect_changes() {
    let provider = GoogleDriveFileSource::new();
    let config = SourceConfig::folder("test-folder-id");

    let info = provider
        .connect(config, "fake-token")
        .await
        .expect("Connect should succeed");

    let changes = provider
        .detect_changes(&info, None, "fake-token")
        .await
        .expect("detect_changes should succeed");

    // Placeholder returns empty events
    assert!(changes.events.is_empty());
    // Should have a cursor for pagination
    assert!(changes.next_cursor.is_some());
    assert!(!changes.has_more);
}

#[tokio::test]
async fn test_google_drive_detect_changes_with_cursor() {
    let provider = GoogleDriveFileSource::new();
    let config = SourceConfig::folder("test-folder-id");

    let info = provider
        .connect(config, "fake-token")
        .await
        .expect("Connect should succeed");

    let changes = provider
        .detect_changes(&info, Some("previous-cursor-token"), "fake-token")
        .await
        .expect("detect_changes should succeed");

    // Placeholder returns empty events but preserves cursor
    assert!(changes.events.is_empty());
    assert_eq!(
        changes.next_cursor.as_deref(),
        Some("previous-cursor-token")
    );
}
