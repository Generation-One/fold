//! User and authentication models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// User role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    Member,
    Viewer,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserRole::Admin => "admin",
            UserRole::Member => "member",
            UserRole::Viewer => "viewer",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(UserRole::Admin),
            "member" => Some(UserRole::Member),
            "viewer" => Some(UserRole::Viewer),
            _ => None,
        }
    }
}

impl Default for UserRole {
    fn default() -> Self {
        UserRole::Member
    }
}

/// A user of the memory system
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,

    /// 'admin', 'member', 'viewer'
    pub role: String,

    // OAuth/OIDC info
    pub provider: Option<String>,
    pub provider_id: Option<String>,

    // Preferences
    pub default_project: Option<String>,

    // Activity
    pub last_active: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    /// Create a new user with generated ID
    pub fn new(username: String, email: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: super::new_id(),
            username,
            email,
            display_name: None,
            avatar_url: None,
            role: UserRole::Member.as_str().to_string(),
            provider: None,
            provider_id: None,
            default_project: None,
            last_active: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Get the typed user role
    pub fn get_role(&self) -> Option<UserRole> {
        UserRole::from_str(&self.role)
    }

    /// Check if user is an admin
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    /// Get display name or fallback to username
    pub fn display(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.username)
    }
}

/// User session for web authentication
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct UserSession {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
}

impl UserSession {
    /// Check if the session has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }
}

/// API token for programmatic access
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct ApiToken {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub token_hash: String,
    /// JSON array of scopes
    pub scopes: Option<String>,
    pub last_used: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ApiToken {
    /// Parse scopes from JSON string
    pub fn scopes_vec(&self) -> Vec<String> {
        self.scopes
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Check if token has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| exp < Utc::now()).unwrap_or(false)
    }

    /// Check if token has a specific scope
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes_vec().iter().any(|s| s == scope || s == "*")
    }
}

/// OIDC state for OAuth flow
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct OidcState {
    pub id: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: Option<String>,
    pub provider: String,
    pub redirect_uri: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl OidcState {
    /// Check if the state has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }
}

/// Webhook registration for git events
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct WebhookRegistration {
    pub id: String,
    pub repository_id: String,
    pub provider: String,
    pub webhook_id: String,
    pub secret: String,
    pub events: String, // JSON array
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WebhookRegistration {
    /// Parse events from JSON string
    pub fn events_vec(&self) -> Vec<String> {
        serde_json::from_str(&self.events).unwrap_or_default()
    }
}
