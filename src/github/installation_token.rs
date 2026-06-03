//! GitHub App installation-token acquisition.
//!
//! This module owns the adapter boundary for short-lived GitHub App
//! installation access tokens. It converts Octocrab's secret token type into a
//! Podbot-owned value that exposes the token string only through an explicit
//! accessor and keeps non-secret expiry metadata available for logging and
//! refresh scheduling.

use std::fmt;
use std::time::{Duration, Instant, SystemTime};

use octocrab::Octocrab;
use secrecy::ExposeSecret;
use tracing::{info, warn};

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
            .ok_or_else(|| GitHubError::TokenAcquisitionFailed {
                message: String::from(
                    "failed to compute installation token metadata: expiry time overflowed",
                ),
            })?;
        Self::from_metadata(token, acquired_at, expires_at, expiry_buffer)
    }

    /// Creates a token value from explicit metadata.
    ///
    /// This constructor is primarily useful for tests and adapter seams that
    /// already know the expiry time.
    ///
    /// # Errors
    ///
    /// Returns [`GitHubError::TokenAcquisitionFailed`] if the expiry or
    /// refresh metadata is not internally consistent.
    pub fn from_metadata(
        token: String,
        acquired_at: SystemTime,
        expires_at: SystemTime,
        expiry_buffer: Duration,
    ) -> Result<Self, GitHubError> {
        if expires_at.duration_since(acquired_at).is_err() {
            return Err(GitHubError::TokenAcquisitionFailed {
                message: String::from(
                    "failed to compute installation token metadata: expiry time precedes acquisition time",
                ),
            });
        }

        let refresh_after = expires_at.checked_sub(expiry_buffer).ok_or_else(|| {
            GitHubError::TokenAcquisitionFailed {
                message: String::from(
                    "failed to compute installation token metadata: refresh time underflowed",
                ),
            }
        })?;

        if refresh_after.duration_since(acquired_at).is_err() {
            return Err(GitHubError::TokenAcquisitionFailed {
                message: String::from(
                    "failed to compute installation token metadata: refresh time precedes acquisition time",
                ),
            });
        }

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

    /// Logs token timing metadata without exposing the token value.
    pub fn log_timing(&self, installation_id: u64, expiry_buffer: Duration) {
        info!(
            installation_id,
            acquired_at = ?self.acquired_at,
            expires_at = ?self.expires_at,
            refresh_after = ?self.refresh_after,
            expiry_buffer_seconds = expiry_buffer.as_secs(),
            "acquired GitHub App installation token"
        );
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

pub(super) async fn acquire_with_octocrab_installation(
    installation: &Octocrab,
    installation_id: u64,
    expiry_buffer: Duration,
) -> Result<InstallationAccessToken, GitHubError> {
    let acquired_at = SystemTime::now();
    let chrono_buffer = chrono::Duration::from_std(expiry_buffer).map_err(|error| {
        GitHubError::TokenAcquisitionFailed {
            message: format!("invalid token expiry buffer: {error}"),
        }
    })?;

    let started_at = Instant::now();
    let secret_result = installation
        .installation_token_with_buffer(chrono_buffer)
        .await;
    let elapsed = started_at.elapsed();
    let secret = match secret_result {
        Ok(secret) => {
            record_token_acquisition_metrics("success", elapsed);
            secret
        }
        Err(error) => {
            record_token_acquisition_metrics("failure", elapsed);
            warn_token_acquisition_failure(installation_id, expiry_buffer, elapsed, &error);
            return Err(classify_token_error(error));
        }
    };
    let token = secret.expose_secret().to_owned();
    let access_token = InstallationAccessToken::new(token, acquired_at, expiry_buffer)?;
    access_token.log_timing(installation_id, expiry_buffer);
    Ok(access_token)
}

fn record_token_acquisition_metrics(status: &'static str, elapsed: Duration) {
    metrics::counter!(
        "podbot.github.installation_token.acquisitions.total",
        "operation" => "installation_token",
        "status" => status,
    )
    .increment(1);
    metrics::histogram!(
        "podbot.github.installation_token.latency_seconds",
        "operation" => "installation_token",
        "status" => status,
    )
    .record(elapsed.as_secs_f64());
}

fn warn_token_acquisition_failure(
    installation_id: u64,
    expiry_buffer: Duration,
    elapsed: Duration,
    error: &octocrab::Error,
) {
    let is_timeout = is_timeout_error(error);
    if is_timeout {
        metrics::counter!(
            "podbot.github.installation_token.timeout_failures.total",
            "operation" => "installation_token",
        )
        .increment(1);
    }
    warn!(
        installation_id,
        expiry_buffer_seconds = expiry_buffer.as_secs(),
        elapsed_seconds = elapsed.as_secs_f64(),
        is_timeout,
        error = %error,
        "failed to acquire GitHub App installation token"
    );
}

fn is_timeout_error(error: &octocrab::Error) -> bool {
    match error {
        octocrab::Error::Service { source, .. } => source
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::TimedOut),
        _ => false,
    }
}

fn classify_token_error(error: octocrab::Error) -> GitHubError {
    let is_github_response = matches!(&error, octocrab::Error::GitHub { .. });
    let message = classify_github_api_error(error).to_string();

    if is_github_response {
        GitHubError::TokenAcquisitionFailed {
            message: format!("GitHub rejected installation token acquisition: {message}"),
        }
    } else {
        GitHubError::TokenAcquisitionFailed { message }
    }
}

#[cfg(test)]
#[path = "installation_token_tests.rs"]
mod tests;
