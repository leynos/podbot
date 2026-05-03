//! Installation-token acquisition for GitHub App integrations.
//!
//! This module exchanges an authenticated GitHub App identity for an
//! installation-scoped access token, preserving expiry metadata so later
//! refresh orchestration can make deterministic decisions.

use std::fmt;
use std::time::Duration;

use camino::Utf8Path;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use jsonwebtoken::EncodingKey;
use octocrab::models::InstallationToken;
use serde::Serialize;

use super::{BoxFuture, OctocrabAppClient, build_app_client, load_private_key};
use crate::error::GitHubError;
use crate::github::classify::classify_installation_token_error;

/// Installation access token plus expiry metadata.
///
/// The token string is preserved for Git operations, while the parsed expiry
/// is retained so later refresh logic can schedule renewals without scraping
/// logs or reparsing API responses.
#[derive(Clone, Eq, PartialEq)]
pub struct InstallationAccessToken {
    token: String,
    expires_at: DateTime<Utc>,
}

impl InstallationAccessToken {
    /// Create a token value object from its constituent parts.
    #[must_use]
    pub const fn new(token: String, expires_at: DateTime<Utc>) -> Self {
        Self { token, expires_at }
    }

    /// Borrow the raw token string for Git operations.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Consume the value object and return the token string.
    #[must_use]
    pub fn into_token(self) -> String {
        self.token
    }

    /// Return the token expiry as a UTC timestamp.
    #[must_use]
    pub const fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

impl fmt::Debug for InstallationAccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InstallationAccessToken")
            .field("token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Trait for installation-token acquisition.
///
/// This seam lets tests inject deterministic token responses without making
/// live GitHub API calls.
#[cfg_attr(test, mockall::automock)]
pub trait GitHubInstallationTokenClient: Send + Sync {
    /// Acquire an installation token for the given installation.
    ///
    /// # Errors
    ///
    /// Returns semantic [`GitHubError`] variants when GitHub rejects the
    /// request or the client fails to communicate with the API.
    fn acquire_installation_token(
        &self,
        installation_id: u64,
    ) -> BoxFuture<'_, Result<InstallationToken, GitHubError>>;
}

impl GitHubInstallationTokenClient for OctocrabAppClient {
    fn acquire_installation_token(
        &self,
        installation_id: u64,
    ) -> BoxFuture<'_, Result<InstallationToken, GitHubError>> {
        Box::pin(async move {
            let route = format!("/app/installations/{installation_id}/access_tokens");
            self.client()
                .post::<_, InstallationToken>(route, Some(&EmptyInstallationTokenRequest {}))
                .await
                .map_err(classify_installation_token_error)
        })
    }
}

/// Inputs required to request an installation token.
#[derive(Clone, Copy, Debug)]
pub struct InstallationTokenRequest<'a> {
    app_id: u64,
    installation_id: u64,
    private_key_path: &'a Utf8Path,
    buffer: Duration,
    now: DateTime<Utc>,
}

impl<'a> InstallationTokenRequest<'a> {
    /// Create a new installation-token request.
    #[must_use]
    pub fn new(
        app_id: u64,
        installation_id: u64,
        private_key_path: &'a Utf8Path,
        buffer: Duration,
    ) -> Self {
        Self {
            app_id,
            installation_id,
            private_key_path,
            buffer,
            now: Utc::now(),
        }
    }

    #[cfg(test)]
    #[must_use]
    pub const fn with_now(mut self, now: DateTime<Utc>) -> Self {
        self.now = now;
        self
    }
}

/// Acquire an installation token whose expiry remains outside the requested
/// buffer.
///
/// This helper loads the configured RSA private key, builds an authenticated
/// GitHub App client, requests an installation token for `installation_id`,
/// and rejects tokens that are already expired or too close to expiry.
///
/// # Errors
///
/// Returns [`GitHubError::PrivateKeyLoadFailed`] if the private key cannot be
/// loaded, [`GitHubError::AuthenticationFailed`] if the App client cannot be
/// constructed, [`GitHubError::TokenAcquisitionFailed`] if GitHub rejects the
/// token request or omits usable expiry metadata, and
/// [`GitHubError::TokenExpired`] if the returned token expires within
/// `buffer`.
pub async fn installation_token_with_buffer(
    app_id: u64,
    installation_id: u64,
    private_key_path: &Utf8Path,
    buffer: Duration,
) -> Result<InstallationAccessToken, GitHubError> {
    installation_token_with_factory(
        InstallationTokenRequest::new(app_id, installation_id, private_key_path, buffer),
        |received_app_id, private_key| {
            let client = build_app_client(received_app_id, private_key)?;
            Ok(OctocrabAppClient::new(client))
        },
    )
    .await
}

/// Acquire an installation token with an injected client factory.
///
/// This helper mirrors [`super::validate_with_factory`], enabling behavioural
/// and unit tests to exercise the full orchestration path without building a
/// live Octocrab client or reaching out to GitHub.
///
/// # Errors
///
/// Returns [`GitHubError::PrivateKeyLoadFailed`] if the private key cannot be
/// loaded, [`GitHubError::AuthenticationFailed`] if the App client cannot be
/// constructed, [`GitHubError::TokenAcquisitionFailed`] if GitHub rejects the
/// token request or omits usable expiry metadata, and
/// [`GitHubError::TokenExpired`] if the returned token expires within the
/// request buffer.
pub async fn installation_token_with_factory<F, C>(
    request: InstallationTokenRequest<'_>,
    factory: F,
) -> Result<InstallationAccessToken, GitHubError>
where
    F: FnOnce(u64, EncodingKey) -> Result<C, GitHubError>,
    C: GitHubInstallationTokenClient,
{
    let private_key = load_private_key(request.private_key_path)?;
    let client = factory(request.app_id, private_key)?;
    installation_token_with_client_and_time(
        &client,
        request.installation_id,
        request.buffer,
        request.now,
    )
    .await
}

pub(crate) async fn installation_token_with_client_and_time(
    client: &dyn GitHubInstallationTokenClient,
    installation_id: u64,
    buffer: Duration,
    now: DateTime<Utc>,
) -> Result<InstallationAccessToken, GitHubError> {
    let token = client.acquire_installation_token(installation_id).await?;
    token_from_response(token, buffer, now)
}

pub(crate) fn token_from_response(
    token: InstallationToken,
    buffer: Duration,
    now: DateTime<Utc>,
) -> Result<InstallationAccessToken, GitHubError> {
    let expires_at = parse_expiry(token.expires_at.as_deref())?;
    ensure_expiry_outside_buffer(expires_at, buffer, now)?;
    Ok(InstallationAccessToken::new(token.token, expires_at))
}

fn parse_expiry(expires_at: Option<&str>) -> Result<DateTime<Utc>, GitHubError> {
    let expiry = expires_at.ok_or_else(|| GitHubError::TokenAcquisitionFailed {
        message: String::from(
            "GitHub did not include expires_at in the installation token response",
        ),
    })?;

    DateTime::parse_from_rfc3339(expiry)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|error| GitHubError::TokenAcquisitionFailed {
            message: format!(
                "GitHub returned an invalid installation token expiry timestamp '{expiry}': {error}"
            ),
        })
}

fn ensure_expiry_outside_buffer(
    expires_at: DateTime<Utc>,
    buffer: Duration,
    now: DateTime<Utc>,
) -> Result<(), GitHubError> {
    let expiry_buffer = chrono_duration(buffer)?;
    if expires_at <= now + expiry_buffer {
        return Err(GitHubError::TokenExpired);
    }
    Ok(())
}

fn chrono_duration(buffer: Duration) -> Result<ChronoDuration, GitHubError> {
    ChronoDuration::from_std(buffer).map_err(|error| GitHubError::TokenAcquisitionFailed {
        message: format!("installation token expiry buffer is invalid: {error}"),
    })
}

#[derive(Debug, Serialize)]
struct EmptyInstallationTokenRequest {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::EmptyInstallationTokenRequest;

    #[test]
    fn empty_installation_token_request_serializes_as_object() {
        let request = EmptyInstallationTokenRequest {};
        let serialized =
            serde_json::to_value(request).expect("empty request should serialize as JSON");
        assert_eq!(serialized, json!({}));
    }
}
