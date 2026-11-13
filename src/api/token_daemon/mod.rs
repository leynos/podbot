//! Token-refresh daemon orchestration.
//!
//! Resolves GitHub App credentials, builds the Octocrab client, and runs the
//! shared refresh loop against a host-side [`TokenStore`]. The store is
//! removed on every exit path so a stopped sandbox leaves no token behind.

use mockable::DefaultClock;

use crate::config::AppConfig;
use crate::error::{ConfigError, PodbotError, Result as PodbotResult};
use crate::github::{
    DaemonShutdown, OctocrabAppClient, RefreshContext, TokenStore, build_app_client,
    load_private_key, run_daemon,
};

use super::CommandOutcome;

/// Run the token refresh daemon for a container.
///
/// The daemon writes refreshed tokens to
/// `$XDG_RUNTIME_DIR/podbot/<container_id>/ghapp_token` and stops when
/// `shutdown` is triggered or token acquisition fails closed.
///
/// # Errors
///
/// Returns configuration errors for incomplete GitHub credentials or a blank
/// container identifier, and GitHub errors when the token cannot be acquired
/// or persisted.
pub fn run_token_daemon(
    config: &AppConfig,
    container_id: &str,
    shutdown: DaemonShutdown,
) -> PodbotResult<CommandOutcome> {
    let validated_id = validate_container_id(container_id)?;
    config.github.validate()?;
    let (app_id, installation_id, private_key_path) = github_credentials(config)?;
    let store = TokenStore::from_env(&mockable::DefaultEnv::new(), validated_id)?;
    let runtime = super::create_runtime()?;

    let daemon_result = runtime.block_on(async {
        let private_key = load_private_key(private_key_path)?;
        let octocrab = build_app_client(app_id, private_key)?;
        let client = OctocrabAppClient::new(octocrab);
        let clock = DefaultClock;
        let context = RefreshContext {
            client: &client,
            installation_id,
            store: &store,
            clock: &clock,
        };
        let mut signal = shutdown.subscribe();
        spawn_shutdown_signals(shutdown);

        run_daemon(&context, &mut signal).await
    });

    finish_with_cleanup(daemon_result, store.cleanup())
}

/// Reject blank container identifiers before any engine or GitHub work.
fn validate_container_id(container_id: &str) -> PodbotResult<&str> {
    if container_id.trim().is_empty() {
        return Err(ConfigError::MissingRequired {
            field: String::from("container_id"),
        }
        .into());
    }
    Ok(container_id)
}

/// Extract validated GitHub credentials.
pub(super) fn github_credentials(
    config: &AppConfig,
) -> PodbotResult<(u64, u64, &camino::Utf8Path)> {
    let app_id = config
        .github
        .app_id
        .ok_or_else(|| ConfigError::MissingRequired {
            field: String::from("github.app_id"),
        })?;
    let installation_id =
        config
            .github
            .installation_id
            .ok_or_else(|| ConfigError::MissingRequired {
                field: String::from("github.installation_id"),
            })?;
    let private_key_path =
        config
            .github
            .private_key_path
            .as_deref()
            .ok_or_else(|| ConfigError::MissingRequired {
                field: String::from("github.private_key_path"),
            })?;

    Ok((app_id, installation_id, private_key_path))
}

/// Stop the daemon when the operator or the service manager asks it to.
///
/// Each signal gets its own watcher, so whichever arrives first requests
/// shutdown and the other simply never fires before the runtime is dropped.
fn spawn_shutdown_signals(shutdown: DaemonShutdown) {
    let ctrl_c = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            ctrl_c.shutdown();
        }
    });
    spawn_sigterm(shutdown);
}

/// Stop the daemon on `SIGTERM`, which is how systemd asks it to exit.
#[cfg(unix)]
fn spawn_sigterm(shutdown: DaemonShutdown) {
    use tokio::signal::unix::{SignalKind, signal};

    tokio::spawn(async move {
        let Ok(mut terminate) = signal(SignalKind::terminate()) else {
            // Registration fails only when the runtime has no signal driver;
            // the Ctrl-C watcher still covers interactive use.
            return;
        };
        // `recv` resolves only when the signal arrives; the value itself
        // carries no information beyond that.
        let _signal = terminate.recv().await;
        shutdown.shutdown();
    });
}

/// Stop the daemon on `SIGTERM` where the platform exposes one.
#[cfg(not(unix))]
const fn spawn_sigterm(_shutdown: DaemonShutdown) {}

/// Preserve the daemon error while still reporting cleanup failures.
fn finish_with_cleanup(
    daemon_result: Result<(), PodbotError>,
    cleanup_result: Result<(), PodbotError>,
) -> PodbotResult<CommandOutcome> {
    match (daemon_result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(CommandOutcome::Success),
        (Ok(()), Err(error)) | (Err(error), Ok(())) => Err(error),
        (Err(error), Err(cleanup_error)) => {
            tracing::warn!(
                %cleanup_error,
                "failed to remove the token store after a daemon failure"
            );
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests;
