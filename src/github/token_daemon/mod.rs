//! Shared token-refresh daemon core.
//!
//! The daemon acquires a GitHub App installation token, writes it to the
//! host-side [`TokenStore`], and refreshes it shortly before expiry. Failed
//! acquisitions are retried with bounded exponential backoff; once the retry
//! budget is exhausted the daemon fails closed instead of serving a stale
//! token. A [`DaemonShutdown`] watch pair lets the CLI stop the loop promptly
//! on Ctrl-C (`SIGINT`).

use std::time::{Duration, SystemTime};

use mockable::Clock;
use tokio::sync::watch;
use tracing::{info, warn};

use crate::error::{GitHubError, PodbotError};
use crate::github::{GitHubInstallationTokenClient, InstallationAccessToken, TokenStore};

/// Refresh tokens five minutes before their reported expiry.
pub const DEFAULT_EXPIRY_BUFFER: Duration = Duration::from_secs(5 * 60);

/// Maximum acquisition attempts before the daemon fails closed.
pub const DEFAULT_MAX_ATTEMPTS: u32 = 5;

/// First retry delay; subsequent delays double up to [`MAX_BACKOFF`].
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);

/// Upper bound for retry delays.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Handle used by the CLI to request daemon shutdown.
#[derive(Debug, Clone)]
pub struct DaemonShutdown {
    sender: watch::Sender<bool>,
}

/// Receiver half of the shutdown watch pair.
#[derive(Debug)]
pub struct DaemonShutdownSignal {
    receiver: watch::Receiver<bool>,
}

impl DaemonShutdown {
    /// Create a shutdown handle.
    #[must_use]
    pub fn new() -> Self {
        let (sender, _receiver) = watch::channel(false);
        Self { sender }
    }

    /// Subscribe a signal receiver to this shutdown handle.
    #[must_use]
    pub fn subscribe(&self) -> DaemonShutdownSignal {
        DaemonShutdownSignal {
            receiver: self.sender.subscribe(),
        }
    }

    /// Request shutdown. Repeated calls are harmless.
    pub fn shutdown(&self) {
        // `send_replace` never fails: it updates the shared value even when
        // every receiver has already been dropped.
        self.sender.send_replace(true);
    }

    /// Return whether shutdown has already been requested.
    #[must_use]
    pub fn is_shutdown(&self) -> bool {
        *self.sender.borrow()
    }
}

impl Default for DaemonShutdown {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonShutdownSignal {
    /// Return whether shutdown has already been requested.
    #[must_use]
    pub fn is_shutdown(&self) -> bool {
        *self.receiver.borrow()
    }

    /// Wait until shutdown is requested, returning immediately if it already
    /// has been.
    async fn wait(&mut self) {
        if self.is_shutdown() {
            return;
        }
        // A value change and a dropped sender both mean the daemon should
        // stop, so the result needs no further handling.
        let _changed = self.receiver.changed().await;
    }
}

/// Borrowed dependencies shared by every refresh cycle.
///
/// Grouping them keeps the daemon entry points within the project's argument
/// limit and makes the stable inputs explicit at the call site.
pub struct RefreshContext<'a> {
    /// Token acquisition port.
    pub client: &'a dyn GitHubInstallationTokenClient,
    /// GitHub App installation identifier.
    pub installation_id: u64,
    /// Host-side store receiving each refreshed token.
    pub store: &'a TokenStore,
    /// Clock used to compute refresh delays.
    pub clock: &'a dyn Clock,
}

/// Run the refresh loop until shutdown or a terminal acquisition failure.
///
/// # Errors
///
/// Returns `GitHubError::DaemonFailed` when acquisition still fails after the
/// retry budget, and `GitHubError::TokenStoreWriteFailed` when the refreshed
/// token cannot be persisted.
pub async fn run_daemon(
    context: &RefreshContext<'_>,
    shutdown: &mut DaemonShutdownSignal,
) -> Result<(), PodbotError> {
    if shutdown.is_shutdown() {
        return Ok(());
    }

    let Some(token) = acquire_with_retries(context, shutdown).await? else {
        return Ok(());
    };
    persist_token(context, &token)?;

    refresh_loop(context, shutdown, token).await
}

/// Continue refreshing from an already-persisted initial token.
///
/// `run_agent` acquires and writes the first token before the container starts
/// so the mounted file is populated when Git first runs. This entry point
/// resumes the same refresh policy from that token instead of acquiring a
/// second one immediately.
///
/// # Errors
///
/// Returns the same errors as [`run_daemon`].
pub async fn run_daemon_with_token(
    context: &RefreshContext<'_>,
    shutdown: &mut DaemonShutdownSignal,
    initial: InstallationAccessToken,
) -> Result<(), PodbotError> {
    refresh_loop(context, shutdown, initial).await
}

/// Sleep until the current token needs refreshing, then replace it.
async fn refresh_loop(
    context: &RefreshContext<'_>,
    shutdown: &mut DaemonShutdownSignal,
    mut token: InstallationAccessToken,
) -> Result<(), PodbotError> {
    while wait_for_refresh(&token, context.clock, shutdown).await {
        let Some(refreshed) = acquire_with_retries(context, shutdown).await? else {
            return Ok(());
        };
        persist_token(context, &refreshed)?;
        token = refreshed;
    }
    Ok(())
}

/// Wait for the refresh deadline, returning `false` when shutdown wins.
async fn wait_for_refresh(
    token: &InstallationAccessToken,
    clock: &dyn Clock,
    shutdown: &mut DaemonShutdownSignal,
) -> bool {
    if shutdown.is_shutdown() {
        return false;
    }
    let delay = refresh_delay(token, clock);
    let shutdown_fired = tokio::time::timeout(delay, shutdown.wait()).await.is_ok();
    !shutdown_fired && !shutdown.is_shutdown()
}

/// Persist a token and record the refresh without exposing its value.
fn persist_token(
    context: &RefreshContext<'_>,
    token: &InstallationAccessToken,
) -> Result<(), PodbotError> {
    context.store.write_token(token.token())?;
    info!(
        installation_id = context.installation_id,
        refresh_after = ?token.refresh_after(),
        "wrote refreshed GitHub App installation token"
    );
    Ok(())
}

/// Acquire a token, retrying transient failures with bounded backoff.
///
/// Returns `None` when shutdown interrupts a retry wait.
async fn acquire_with_retries(
    context: &RefreshContext<'_>,
    shutdown: &mut DaemonShutdownSignal,
) -> Result<Option<InstallationAccessToken>, PodbotError> {
    let mut attempt = 0;

    loop {
        attempt += 1;
        match acquire_once(context).await {
            Ok(token) => return Ok(Some(token)),
            Err(error) => {
                if attempt >= DEFAULT_MAX_ATTEMPTS {
                    return Err(daemon_failed(attempt, &error));
                }
                warn_retry(context.installation_id, attempt, &error);
                if wait_for_retry(attempt, shutdown).await {
                    return Ok(None);
                }
            }
        }
    }
}

/// Acquire a single installation token through the injected client.
async fn acquire_once(
    context: &RefreshContext<'_>,
) -> Result<InstallationAccessToken, PodbotError> {
    context
        .client
        .acquire_installation_token(context.installation_id, DEFAULT_EXPIRY_BUFFER)
        .await
        .map_err(PodbotError::from)
}

/// Report a failed attempt without exposing token material.
fn warn_retry(installation_id: u64, attempt: u32, error: &PodbotError) {
    warn!(
        installation_id,
        attempt,
        max_attempts = DEFAULT_MAX_ATTEMPTS,
        error = %error,
        "token acquisition failed; retrying"
    );
}

/// Wait out the backoff, returning `true` when shutdown interrupts it.
async fn wait_for_retry(attempt: u32, shutdown: &mut DaemonShutdownSignal) -> bool {
    let shutdown_fired = tokio::time::timeout(backoff_delay(attempt), shutdown.wait())
        .await
        .is_ok();
    shutdown_fired || shutdown.is_shutdown()
}

/// Build the terminal failure reported once the retry budget is exhausted.
fn daemon_failed(attempt: u32, error: &PodbotError) -> PodbotError {
    GitHubError::DaemonFailed {
        message: format!("token acquisition failed after {attempt} attempts: {error}"),
    }
    .into()
}

/// Exponential backoff capped at [`MAX_BACKOFF`].
fn backoff_delay(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(5);
    let factor = 1_u32 << exponent;
    (INITIAL_BACKOFF * factor).min(MAX_BACKOFF)
}

/// Time until the token should be refreshed, never negative.
fn refresh_delay(token: &InstallationAccessToken, clock: &dyn Clock) -> Duration {
    let now: SystemTime = clock.utc().into();
    token
        .refresh_after()
        .duration_since(now)
        .unwrap_or(Duration::ZERO)
}

#[cfg(test)]
mod tests;
