//! Claude Code credentials reader.
//!
//! Reads OAuth tokens from Claude Code's ~/.claude/.credentials.json file.
//! This allows Fold to use the same authentication as Claude Code for
//! users with a Max/Pro subscription.
//!
//! Note: This is a grey area usage - the tokens are obtained through
//! Claude Code's OAuth flow, not through a dedicated API key.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::error::{Error, Result};

/// Claude Code OAuth credentials structure.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeOAuth {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
}

/// Full credentials file structure.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeCredentials {
    pub claude_ai_oauth: Option<ClaudeCodeOAuth>,
    pub organization_uuid: Option<String>,
}

impl ClaudeCodeCredentials {
    /// Check if the OAuth token is expired.
    pub fn is_expired(&self) -> bool {
        if let Some(ref oauth) = self.claude_ai_oauth {
            if let Some(ref expires_at) = oauth.expires_at {
                if let Ok(expiry) = DateTime::parse_from_rfc3339(expires_at) {
                    return Utc::now() > expiry.with_timezone(&Utc);
                }
            }
        }
        false
    }

    /// Get the access token if available and not expired.
    pub fn access_token(&self) -> Option<&str> {
        if self.is_expired() {
            warn!("Claude Code OAuth token is expired");
            return None;
        }
        self.claude_ai_oauth
            .as_ref()
            .map(|o| o.access_token.as_str())
    }

    /// Get the subscription type (e.g., "max", "pro").
    pub fn subscription_type(&self) -> Option<&str> {
        self.claude_ai_oauth
            .as_ref()
            .and_then(|o| o.subscription_type.as_deref())
    }
}

/// Service for reading Claude Code credentials.
#[derive(Clone)]
pub struct ClaudeCodeService {
    credentials_path: PathBuf,
}

impl ClaudeCodeService {
    /// Create a new Claude Code service.
    pub fn new() -> Self {
        Self {
            credentials_path: Self::default_credentials_path(),
        }
    }

    /// Create with a custom credentials path.
    pub fn with_path(path: PathBuf) -> Self {
        Self {
            credentials_path: path,
        }
    }

    /// Get the default credentials path.
    fn default_credentials_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".claude").join(".credentials.json")
    }

    /// Check if credentials file exists.
    pub fn credentials_exist(&self) -> bool {
        self.credentials_path.exists()
    }

    /// Read credentials from the file.
    pub fn read_credentials(&self) -> Result<ClaudeCodeCredentials> {
        if !self.credentials_exist() {
            return Err(Error::Internal(format!(
                "Claude Code credentials not found at {}",
                self.credentials_path.display()
            )));
        }

        let content = std::fs::read_to_string(&self.credentials_path)
            .map_err(|e| Error::Internal(format!("Failed to read credentials: {}", e)))?;

        let creds: ClaudeCodeCredentials = serde_json::from_str(&content)
            .map_err(|e| Error::Internal(format!("Failed to parse credentials: {}", e)))?;

        debug!(
            subscription = ?creds.subscription_type(),
            expired = creds.is_expired(),
            "Read Claude Code credentials"
        );

        Ok(creds)
    }

    /// Get the current access token if available.
    pub fn get_access_token(&self) -> Result<String> {
        let creds = self.read_credentials()?;
        creds
            .access_token()
            .map(|s| s.to_string())
            .ok_or_else(|| Error::Internal("No valid Claude Code token available".to_string()))
    }

    /// Check if Claude Code authentication is available and valid.
    pub fn is_available(&self) -> bool {
        if !self.credentials_exist() {
            return false;
        }
        match self.read_credentials() {
            Ok(creds) => creds.access_token().is_some(),
            Err(_) => false,
        }
    }

    /// Get credentials info for display (without sensitive data).
    pub fn get_info(&self) -> Option<ClaudeCodeInfo> {
        self.read_credentials().ok().map(|creds| ClaudeCodeInfo {
            subscription_type: creds.subscription_type().map(String::from),
            is_expired: creds.is_expired(),
            has_token: creds.claude_ai_oauth.is_some(),
            organization_id: creds.organization_uuid.clone(),
        })
    }
}

impl Default for ClaudeCodeService {
    fn default() -> Self {
        Self::new()
    }
}

/// Non-sensitive info about Claude Code credentials.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClaudeCodeInfo {
    pub subscription_type: Option<String>,
    pub is_expired: bool,
    pub has_token: bool,
    pub organization_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credentials_path() {
        let service = ClaudeCodeService::new();
        let path = service.credentials_path.to_string_lossy();
        assert!(path.contains(".claude"));
        assert!(path.ends_with(".credentials.json"));
    }
}
