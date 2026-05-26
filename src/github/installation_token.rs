//! GitHub App installation-token acquisition.
//!
//! This module owns the adapter boundary for short-lived GitHub App
//! installation access tokens. It converts Octocrab's secret token type into a
//! Podbot-owned value that exposes the token string only through an explicit
//! accessor and keeps non-secret expiry metadata available for logging and
//! refresh scheduling.

use std::fmt;
use std::time::{Duration, SystemTime};

use octocrab::Octocrab;
use secrecy::ExposeSecret;
use tracing::info;

use crate::error::GitHubError;
use crate::github::classify::classify_github_api_error;

const GITHUB_INSTALLATION_TOKEN_LIFETIME: Duration = Duration::from_secs(60 * 60);

/// A GitHub App installation token and its non-secret scheduling metadata.
#[derive(Clone, Eq, PartialEq)]
pub struct InstallationAccessToken {
    token: String,
    acquired_at: SystemTime,
    expires_at: SystemTime,
    refresh_after: SystemTime,
}

impl InstallationAccessToken {
    /// Creates a token value from an explicit token string and timing metadata.
    ///
    /// # Errors
    ///
    /// Returns [`GitHubError::TokenAcquisitionFailed`] if the expiry time or
    /// refresh time cannot be represented for the supplied clock values.
    pub fn new(
        token: String,
        acquired_at: SystemTime,
        expiry_buffer: Duration,
    ) -> Result<Self, GitHubError> {
        let expires_at = acquired_at
            .checked_add(GITHUB_INSTALLATION_TOKEN_LIFETIME)
            .ok_or_else(|| token_metadata_error("expiry time overflowed"))?;
        Self::from_metadata(token, acquired_at, expires_at, expiry_buffer)
    }

    /// Creates a token value from explicit metadata.
    ///
    /// This constructor is primarily useful for tests and adapter seams that
    /// already know the expiry time.
    ///
    /// # Errors
    ///
    /// Returns [`GitHubError::TokenAcquisitionFailed`] if `refresh_after`
    /// would be earlier than the representable [`SystemTime`] range.
    pub fn from_metadata(
        token: String,
        acquired_at: SystemTime,
        expires_at: SystemTime,
        expiry_buffer: Duration,
    ) -> Result<Self, GitHubError> {
        let refresh_after = expires_at
            .checked_sub(expiry_buffer)
            .ok_or_else(|| token_metadata_error("refresh time underflowed"))?;
        Ok(Self {
            token,
            acquired_at,
            expires_at,
            refresh_after,
        })
    }

    /// Returns the token string for Git credential delivery.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Returns when Podbot acquired the token.
    #[must_use]
    pub const fn acquired_at(&self) -> SystemTime {
        self.acquired_at
    }

    /// Returns the conservative expiry time for refresh scheduling.
    #[must_use]
    pub const fn expires_at(&self) -> SystemTime {
        self.expires_at
    }

    /// Returns when refresh should begin, after applying the configured buffer.
    #[must_use]
    pub const fn refresh_after(&self) -> SystemTime {
        self.refresh_after
    }

    /// Returns non-secret fields suitable for structured logging.
    #[must_use]
    pub const fn log_fields(
        &self,
        installation_id: u64,
        expiry_buffer: Duration,
    ) -> InstallationTokenLogFields {
        InstallationTokenLogFields {
            installation_id,
            acquired_at: self.acquired_at,
            expires_at: self.expires_at,
            refresh_after: self.refresh_after,
            expiry_buffer,
        }
    }
}

impl fmt::Debug for InstallationAccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InstallationAccessToken")
            .field("token", &"<redacted>")
            .field("acquired_at", &self.acquired_at)
            .field("expires_at", &self.expires_at)
            .field("refresh_after", &self.refresh_after)
            .finish()
    }
}

/// Non-secret token acquisition metadata suitable for logs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstallationTokenLogFields {
    /// GitHub App installation ID used to acquire the token.
    pub installation_id: u64,
    /// Host clock time sampled when token acquisition began.
    pub acquired_at: SystemTime,
    /// Conservative expiry time derived from GitHub's one-hour token lifetime.
    pub expires_at: SystemTime,
    /// Time at which refresh should begin.
    pub refresh_after: SystemTime,
    /// Buffer required before expiry.
    pub expiry_buffer: Duration,
}

/// Logs installation token timing without exposing the token value.
pub fn log_installation_token_timing(fields: InstallationTokenLogFields) {
    info!(
        installation_id = fields.installation_id,
        acquired_at = ?fields.acquired_at,
        expires_at = ?fields.expires_at,
        refresh_after = ?fields.refresh_after,
        expiry_buffer_seconds = fields.expiry_buffer.as_secs(),
        "acquired GitHub App installation token"
    );
}

pub(super) async fn acquire_with_octocrab_installation(
    installation: &Octocrab,
    installation_id: u64,
    expiry_buffer: Duration,
    acquired_at: SystemTime,
) -> Result<InstallationAccessToken, GitHubError> {
    let chrono_buffer = chrono::Duration::from_std(expiry_buffer).map_err(|error| {
        GitHubError::TokenAcquisitionFailed {
            message: format!("invalid token expiry buffer: {error}"),
        }
    })?;

    let secret = installation
        .installation_token_with_buffer(chrono_buffer)
        .await
        .map_err(classify_token_error)?;
    let token = secret.expose_secret().to_owned();
    let access_token = InstallationAccessToken::new(token, acquired_at, expiry_buffer)?;
    log_installation_token_timing(access_token.log_fields(installation_id, expiry_buffer));
    Ok(access_token)
}

fn classify_token_error(error: octocrab::Error) -> GitHubError {
    match classify_github_api_error(error) {
        GitHubError::AuthenticationFailed { message } => GitHubError::TokenAcquisitionFailed {
            message: format!("GitHub rejected installation token acquisition: {message}"),
        },
        other => GitHubError::TokenAcquisitionFailed {
            message: other.to_string(),
        },
    }
}

fn token_metadata_error(message: &str) -> GitHubError {
    GitHubError::TokenAcquisitionFailed {
        message: format!("failed to compute installation token metadata: {message}"),
    }
}

#[cfg(test)]
#[path = "installation_token_tests.rs"]
mod tests;
