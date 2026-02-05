//! Error types for Fold.
//!
//! Uses thiserror for ergonomic error definitions that integrate
//! with axum's response system.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // Auth errors
    #[error("Not authenticated")]
    Unauthenticated,

    #[error("Insufficient permissions")]
    Forbidden,

    #[error("Invalid token")]
    InvalidToken,

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid credentials")]
    InvalidCredentials,

    // Resource errors
    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Resource already exists: {0}")]
    AlreadyExists(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    // Validation errors
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    // External service errors
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Vector store error: {0}")]
    VectorStore(String),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("GitHub API error: {0}")]
    GitHub(String),

    #[error("GitLab API error: {0}")]
    GitLab(String),

    #[error("Webhook error: {0}")]
    Webhook(String),

    // File errors
    #[error("File too large: max {max_size} bytes")]
    FileTooLarge { max_size: usize },

    #[error("Invalid file type: {0}")]
    InvalidFileType(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    // Rate limiting
    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    // Generic errors
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl Error {
    pub fn status_code(&self) -> StatusCode {
        match self {
            // 401
            Self::Unauthenticated | Self::InvalidToken | Self::TokenExpired => {
                StatusCode::UNAUTHORIZED
            }

            // 403
            Self::Forbidden | Self::InvalidCredentials => StatusCode::FORBIDDEN,

            // 404
            Self::NotFound(_) | Self::FileNotFound(_) => StatusCode::NOT_FOUND,

            // 409
            Self::AlreadyExists(_) | Self::Conflict(_) => StatusCode::CONFLICT,

            // 400
            Self::Validation(_) | Self::InvalidInput(_) | Self::InvalidFileType(_) => {
                StatusCode::BAD_REQUEST
            }

            // 413
            Self::FileTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,

            // 429
            Self::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,

            // 501
            Self::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,

            // 502
            Self::GitHub(_) | Self::GitLab(_) | Self::Llm(_) => StatusCode::BAD_GATEWAY,

            // 500
            Self::Database(_)
            | Self::VectorStore(_)
            | Self::Embedding(_)
            | Self::Webhook(_)
            | Self::Internal(_)
            | Self::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Unauthenticated => "UNAUTHENTICATED",
            Self::Forbidden => "FORBIDDEN",
            Self::InvalidToken => "INVALID_TOKEN",
            Self::TokenExpired => "TOKEN_EXPIRED",
            Self::InvalidCredentials => "INVALID_CREDENTIALS",
            Self::NotFound(_) => "NOT_FOUND",
            Self::AlreadyExists(_) => "ALREADY_EXISTS",
            Self::Conflict(_) => "CONFLICT",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::InvalidInput(_) => "INVALID_INPUT",
            Self::Database(_) => "DATABASE_ERROR",
            Self::VectorStore(_) => "VECTOR_STORE_ERROR",
            Self::Embedding(_) => "EMBEDDING_ERROR",
            Self::Llm(_) => "LLM_ERROR",
            Self::GitHub(_) => "GITHUB_ERROR",
            Self::GitLab(_) => "GITLAB_ERROR",
            Self::Webhook(_) => "WEBHOOK_ERROR",
            Self::FileTooLarge { .. } => "FILE_TOO_LARGE",
            Self::InvalidFileType(_) => "INVALID_FILE_TYPE",
            Self::FileNotFound(_) => "FILE_NOT_FOUND",
            Self::RateLimitExceeded => "RATE_LIMIT_EXCEEDED",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::NotImplemented(_) => "NOT_IMPLEMENTED",
            Self::Other(_) => "UNKNOWN_ERROR",
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = self.error_code();
        let message = self.to_string();

        let body = Json(json!({
            "error": {
                "code": code,
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}

// Convenience conversions
impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Internal(err.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::Internal(format!("HTTP request failed: {}", err))
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::InvalidInput(format!("JSON parsing error: {}", err))
    }
}
