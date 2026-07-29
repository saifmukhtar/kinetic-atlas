//! Unified error taxonomy and logbook registry for Kinetic Atlas.
//!
//! All errors in Kinetic Atlas follow a domain-specific hierarchy to prevent
//! raw OS error leakage, enforce RFC 7807 problem details URIs, and supply clean
//! user-facing messages.

use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Severity level for logging and monitoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Expected or informational conditions.
    Info,
    /// Unexpected but recoverable conditions (e.g., node offline).
    Warning,
    /// Unrecoverable conditions or internal logic failures.
    Error,
    /// Critical failures requiring immediate operator intervention.
    Critical,
}

/// Top-level error type for Kinetic Atlas node operations.
#[derive(Error, Debug)]
pub enum AtlasError {
    /// General network or libp2p swarm initialization error.
    #[error("Swarm network initialization failed: {0}")]
    NetworkInitFailed(String),

    /// DNS Tree resolution error.
    #[error("Failed to resolve DNS tree: {0}")]
    DnsResolutionFailed(String),

    /// Missing or invalid configuration.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Proxy request failed or was malformed.
    #[error("Invalid proxy request: {0}")]
    InvalidProxyRequest(String),

    /// Proxy target (e.g. IPFS) failed to respond.
    #[error("All proxy targets failed for domain")]
    ProxyTargetFailed,

    /// Updater failed to sync the registry.
    #[error("Auto-updater failed to fetch from GitHub: {0}")]
    UpdaterFailed(String),

    /// TLD is blacklisted.
    #[error("The requested TLD is blacklisted")]
    TldBlacklisted,
}

impl AtlasError {
    /// Stable protocol error code for Kinetic Atlas.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NetworkInitFailed(_) => "KIN-ATL-001",
            Self::DnsResolutionFailed(_) => "KIN-ATL-002",
            Self::ConfigError(_) => "KIN-ATL-003",
            Self::InvalidProxyRequest(_) => "KIN-ATL-004",
            Self::ProxyTargetFailed => "KIN-ATL-005",
            Self::UpdaterFailed(_) => "KIN-ATL-006",
            Self::TldBlacklisted => "KIN-ATL-007",
        }
    }

    /// RFC 7807 type URI for this error.
    pub fn error_type_uri(&self) -> String {
        format!("https://docs.kinetic.network/errors/{}", self.code())
    }

    /// Whether the client should offer a retry action.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::NetworkInitFailed(_)
                | Self::DnsResolutionFailed(_)
                | Self::UpdaterFailed(_)
                | Self::ProxyTargetFailed
        )
    }

    /// Severity level for logging and monitoring.
    pub fn severity(&self) -> Severity {
        match self {
            Self::NetworkInitFailed(_) => Severity::Error,
            Self::DnsResolutionFailed(_) => Severity::Warning,
            Self::ConfigError(_) => Severity::Critical,
            Self::InvalidProxyRequest(_) => Severity::Info,
            Self::ProxyTargetFailed => Severity::Warning,
            Self::UpdaterFailed(_) => Severity::Error,
            Self::TldBlacklisted => Severity::Info,
        }
    }

    /// Clean, user-facing explanation of the error.
    pub fn user_message(&self) -> String {
        self.to_string()
    }

    /// Developer-facing structured JSON diagnostic details.
    pub fn details(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

/// RFC 7807 Problem Details representation for HTTP API responses, augmented with Kinetic extensions.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiError {
    /// RFC 7807: URI identifying the specific error category.
    #[serde(rename = "type")]
    pub error_type: String,
    /// RFC 7807: Short human-readable title summarizing the error category.
    pub title: String,
    /// RFC 7807: Associated HTTP response status code (e.g. `404`, `503`).
    pub status: u16,
    /// RFC 7807: Human-facing explanation of the specific error occurrence.
    pub detail: String,
    /// RFC 7807: Optional URI identifying the specific request instance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// Kinetic Extension: Stable protocol error code (e.g. `"KIN-ATL-002"`).
    pub code: String,
    /// Kinetic Extension: Indicates whether client applications should retry the request.
    pub retryable: bool,
    /// Kinetic Extension: Developer-facing structured JSON diagnostic details.
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
    /// Kinetic Extension: Task-local correlation ID for server log tracing.
    pub request_id: String,
}

impl ApiError {
    /// Returns the HTTP status code associated with this error.
    pub fn http_status(&self) -> u16 {
        self.status
    }
}

fn current_request_id() -> String {
    // Atlas doesn't have a complex tracing span ID extractor right now, so we generate a random one or use a placeholder.
    "atlas-req-id-placeholder".to_string()
}

impl From<AtlasError> for ApiError {
    fn from(e: AtlasError) -> Self {
        let (status, title): (u16, &'static str) = match &e {
            AtlasError::NetworkInitFailed(_) => (500, "Network Initialization Failed"),
            AtlasError::DnsResolutionFailed(_) => (502, "DNS Resolution Failed"),
            AtlasError::ConfigError(_) => (500, "Server Configuration Error"),
            AtlasError::InvalidProxyRequest(_) => (400, "Invalid Proxy Request"),
            AtlasError::ProxyTargetFailed => (502, "Bad Gateway"),
            AtlasError::UpdaterFailed(_) => (500, "Auto-Updater Error"),
            AtlasError::TldBlacklisted => (403, "Domain Blacklisted"),
        };
        ApiError {
            error_type: e.error_type_uri(),
            title: title.to_string(),
            status,
            detail: e.user_message(),
            instance: None,
            code: e.code().to_string(),
            retryable: e.is_retryable(),
            details: e.details(),
            request_id: current_request_id(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = axum::http::StatusCode::from_u16(self.status)
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::Json(self);
        (status, body).into_response()
    }
}

impl IntoResponse for AtlasError {
    fn into_response(self) -> Response {
        let api_err: ApiError = self.into();
        api_err.into_response()
    }
}
