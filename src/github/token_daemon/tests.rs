//! Unit tests for the token-refresh daemon core.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use camino::Utf8Path;
use mockable::DefaultClock;
use mockall::mock;
use tempfile::TempDir;

use super::*;
use crate::error::GitHubError;

mock! {
    TokenClient {}
    impl GitHubInstallationTokenClient for TokenClient {
        fn acquire_installation_token(
            &self,
            installation_id: u64,
            expiry_buffer: Duration,
        ) -> crate::github::BoxFuture<'_, Result<InstallationAccessToken, GitHubError>>;
    }
}

fn token() -> InstallationAccessToken {
    let now = SystemTime::now();
    InstallationAccessToken::from_metadata(
        String::from("token-value"),
        now,
        now + Duration::from_secs(60 * 60),
        DEFAULT_EXPIRY_BUFFER,
    )
    .expect("token metadata should be valid")
}

fn store(runtime_dir: &TempDir) -> TokenStore {
    let path = Utf8Path::from_path(runtime_dir.path()).expect("temp path should be UTF-8");
    TokenStore::new(path, "session-1").expect("token store should be created")
}

#[tokio::test(start_paused = true)]
async fn daemon_writes_token_and_refreshes_before_expiry() {
    let runtime_dir = tempfile::tempdir().expect("temp dir should be created");
    let store = store(&runtime_dir);
    let shutdown = DaemonShutdown::new();
    let mut signal = shutdown.subscribe();
    let shutdown_for_refresh = shutdown.clone();
    let mut client = MockTokenClient::new();

    client
        .expect_acquire_installation_token()
        .times(1)
        .returning(|_, _| Box::pin(async { Ok(token()) }));
    client
        .expect_acquire_installation_token()
        .times(1)
        .returning(move |_, _| {
            let refresh_shutdown = shutdown_for_refresh.clone();
            Box::pin(async move {
                refresh_shutdown.shutdown();
                Ok(token())
            })
        });

    let handle = tokio::spawn(async move {
        let clock = DefaultClock;
        let context = RefreshContext {
            client: &client,
            installation_id: 42,
            store: &store,
            clock: &clock,
        };
        run_daemon(&context, &mut signal).await
    });

    tokio::time::advance(Duration::from_secs(55 * 60)).await;
    handle
        .await
        .expect("daemon task should join")
        .expect("daemon should stop cleanly");
}

#[tokio::test(start_paused = true)]
async fn daemon_retries_transient_failures() {
    let runtime_dir = tempfile::tempdir().expect("temp dir should be created");
    let store = store(&runtime_dir);
    let shutdown = DaemonShutdown::new();
    let mut signal = shutdown.subscribe();
    let shutdown_for_success = shutdown.clone();
    let mut client = MockTokenClient::new();

    client
        .expect_acquire_installation_token()
        .times(1)
        .returning(|_, _| {
            Box::pin(async {
                Err(GitHubError::TokenAcquisitionFailed {
                    message: String::from("temporary outage"),
                })
            })
        });
    client
        .expect_acquire_installation_token()
        .times(1)
        .returning(move |_, _| {
            let success_shutdown = shutdown_for_success.clone();
            Box::pin(async move {
                success_shutdown.shutdown();
                Ok(token())
            })
        });

    let store_for_task = store.clone();
    let handle = tokio::spawn(async move {
        let clock = DefaultClock;
        let context = RefreshContext {
            client: &client,
            installation_id: 42,
            store: &store_for_task,
            clock: &clock,
        };
        run_daemon(&context, &mut signal).await
    });

    handle
        .await
        .expect("daemon task should join")
        .expect("daemon should recover after a retry");
    assert!(
        store.token_path().exists(),
        "retry success should write the token"
    );
}

#[tokio::test(start_paused = true)]
async fn daemon_fails_closed_after_exhausting_retries() {
    let runtime_dir = tempfile::tempdir().expect("temp dir should be created");
    let store = store(&runtime_dir);
    let shutdown = DaemonShutdown::new();
    let mut signal = shutdown.subscribe();
    let mut client = MockTokenClient::new();

    client
        .expect_acquire_installation_token()
        .times(DEFAULT_MAX_ATTEMPTS as usize)
        .returning(|_, _| {
            Box::pin(async {
                Err(GitHubError::TokenAcquisitionFailed {
                    message: String::from("permanent outage"),
                })
            })
        });

    let clock = DefaultClock;
    let context = RefreshContext {
        client: &client,
        installation_id: 42,
        store: &store,
        clock: &clock,
    };
    let result = run_daemon(&context, &mut signal).await;

    assert!(
        matches!(
            result,
            Err(PodbotError::GitHub(GitHubError::DaemonFailed { .. }))
        ),
        "expected a terminal daemon failure, got {result:?}"
    );
    assert!(
        !store.token_path().exists(),
        "failed acquisition must not write a token"
    );
}

#[tokio::test]
async fn shutdown_before_start_skips_acquisition() {
    let runtime_dir = tempfile::tempdir().expect("temp dir should be created");
    let store = store(&runtime_dir);
    let shutdown = DaemonShutdown::new();
    let mut signal = shutdown.subscribe();
    shutdown.shutdown();
    let mut client = MockTokenClient::new();
    client.expect_acquire_installation_token().times(0);

    let clock = DefaultClock;
    let context = RefreshContext {
        client: &client,
        installation_id: 42,
        store: &store,
        clock: &clock,
    };
    run_daemon(&context, &mut signal)
        .await
        .expect("pre-requested shutdown should return success");
}

#[tokio::test(start_paused = true)]
async fn shutdown_interrupts_retry_backoff() {
    let runtime_dir = tempfile::tempdir().expect("temp dir should be created");
    let store = store(&runtime_dir);
    let shutdown = DaemonShutdown::new();
    let mut signal = shutdown.subscribe();
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_mock = Arc::clone(&attempts);
    let mut client = MockTokenClient::new();

    client
        .expect_acquire_installation_token()
        .times(1)
        .returning(move |_, _| {
            attempts_for_mock.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Err(GitHubError::TokenAcquisitionFailed {
                    message: String::from("temporary outage"),
                })
            })
        });

    let handle = tokio::spawn(async move {
        let clock = DefaultClock;
        let context = RefreshContext {
            client: &client,
            installation_id: 42,
            store: &store,
            clock: &clock,
        };
        run_daemon(&context, &mut signal).await
    });

    while attempts.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    shutdown.shutdown();

    handle
        .await
        .expect("daemon task should join")
        .expect("shutdown during backoff should stop cleanly");
}

#[test]
fn backoff_doubles_up_to_the_cap() {
    assert_eq!(backoff_delay(1), Duration::from_secs(1));
    assert_eq!(backoff_delay(2), Duration::from_secs(2));
    assert_eq!(backoff_delay(3), Duration::from_secs(4));
    assert_eq!(backoff_delay(10), MAX_BACKOFF);
}
