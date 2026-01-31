//! Middleware for Fold.
//!
//! Provides authentication and authorization middleware:
//! - `token_auth` - API token validation for programmatic access (MCP, CLI, webhooks)
//! - `session_auth` - Session/cookie validation for web UI access

mod session_auth;
mod token_auth;

pub use session_auth::{
    optional_session, require_admin, require_session, SessionUser, SESSION_COOKIE_NAME,
};
pub use token_auth::{
    require_project_access, require_project_member, require_project_write, require_token,
    AuthContext, ProjectAccess,
};
