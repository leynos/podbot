//! Shared test utilities for integration tests.
//!
//! This module provides common helpers used across integration tests, particularly
//! for environment variable manipulation which requires careful synchronization.

#![cfg(feature = "internal")]

use std::sync::{Mutex, MutexGuard};

use podbot::api::{CommandOutcome, ExecRequest};
use podbot::config::env_var_names;
use podbot::engine::{ContainerExecClient, EngineConnector};
use rstest::{fixture, rstest};

/// Global mutex protecting environment variable access.
///
/// All tests that read or modify environment variables must acquire this lock
/// to prevent data races. While `#[serial]` prevents concurrent test execution,
/// this mutex provides an explicit guard pattern that makes the synchronization
/// visible in test code and ensures cleanup on drop.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard for exclusive environment variable access.
///
/// Acquiring an `EnvGuard` locks the global environment mutex, ensuring no other
/// test can read or modify environment variables until the guard is dropped.
///
/// # Example
///
/// ```ignore
/// let _guard = EnvGuard::lock();
/// // Environment is now exclusively ours
/// unsafe { std::env::set_var("FOO", "bar"); }
/// // Guard dropped here, releasing the lock
/// ```
pub struct EnvGuard<'a> {
    _guard: MutexGuard<'a, ()>,
}

impl EnvGuard<'_> {
    /// Acquire exclusive access to the environment.
    ///
    /// Blocks until the lock is available. If the mutex is poisoned (a previous
    /// holder panicked), the lock is still acquired to allow tests to continue.
    #[must_use]
    pub fn lock() -> EnvGuard<'static> {
        let guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        EnvGuard { _guard: guard }
    }
}

/// Clears all `PODBOT_*` environment variables and returns a guard.
///
/// This function acquires the environment mutex, clears all known podbot
/// environment variables, and returns a guard that holds the lock. The caller
/// must keep the guard alive for the duration of the test to maintain exclusive
/// environment access.
///
/// Uses [`env_var_names()`] from the configuration module to stay in sync with
/// the actual environment variable mappings. Also clears `PODBOT_CONFIG_PATH`
/// which is handled separately by the config discovery mechanism.
///
/// # Safety
///
/// The unsafe blocks are required because `std::env::remove_var` is unsafe in
/// Rust 2024 edition. Safety is ensured by the mutex guard which guarantees
/// exclusive access to environment variables.
///
/// # Example
///
/// ```ignore
/// #[test]
/// #[serial]
/// fn my_test() {
///     let _guard = clear_podbot_env();
///     // Environment is clean and exclusively ours
///     // ... test code ...
/// } // Guard dropped, lock released
/// ```
#[must_use]
pub fn clear_podbot_env() -> EnvGuard<'static> {
    let guard = EnvGuard::lock();

    // Clear the config path env var (handled by config discovery, not ENV_VAR_SPECS).
    // SAFETY: Mutex guard ensures exclusive access to environment variables.
    unsafe {
        std::env::remove_var("PODBOT_CONFIG_PATH");
    }

    // Clear all env vars from the loader's specification table.
    for var in env_var_names() {
        // SAFETY: Mutex guard ensures exclusive access to environment variables.
        unsafe {
            std::env::remove_var(var);
        }
    }

    guard
}

/// Sets an environment variable while holding the environment guard.
///
/// This function requires a reference to an `EnvGuard` to ensure the caller
/// has exclusive access to environment variables. This prevents accidental
/// use without proper synchronization.
///
/// # Safety
///
/// The unsafe block is required because `std::env::set_var` is unsafe in
/// Rust 2024 edition. Safety is ensured by the guard reference which guarantees
/// exclusive access to environment variables.
///
/// # Example
///
/// ```ignore
/// let guard = clear_podbot_env();
/// set_env_var(&guard, "PODBOT_SANDBOX_PRIVILEGED", "true");
/// // ... test code using the env var ...
/// ```
pub fn set_env_var(_guard: &EnvGuard<'_>, key: &str, value: &str) {
    // SAFETY: The guard reference ensures exclusive access to environment variables.
    unsafe {
        std::env::set_var(key, value);
    }
}

/// Execute an API `ExecRequest` against a mock engine client in integration tests.
///
/// This keeps the engine-to-API translation in one place for integration tests
/// that must stay outside the crate-private `podbot::api::exec_with_client`
/// seam.
///
/// # Errors
///
/// Returns any validation or engine-exec error produced while translating the
/// API request into an engine request and running it through the provided mock
/// client.
pub fn exec_outcome_with_client<C: ContainerExecClient + Sync>(
    client: &C,
    runtime: &tokio::runtime::Handle,
    request: &ExecRequest,
) -> podbot::error::Result<CommandOutcome> {
    let engine_request = podbot::engine::ExecRequest::new(
        request.container(),
        request.command().to_vec(),
        request.mode().into(),
    )?
    .with_tty(request.tty());
    let result = EngineConnector::exec(runtime, client, &engine_request)?;

    if result.exit_code() == 0 {
        Ok(CommandOutcome::Success)
    } else {
        Ok(CommandOutcome::CommandExit {
            code: result.exit_code(),
        })
    }
}

const DISABLE_STDIN_FORWARDING_ENV: &str = "PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS";

/// RAII guard that disables stdin forwarding for exec integration tests.
pub struct TestStdinForwardingGuard {
    _env_guard: EnvGuard<'static>,
}

impl TestStdinForwardingGuard {
    /// Disable host stdin forwarding while holding the shared environment lock.
    #[must_use]
    pub fn disable() -> Self {
        let env_guard = EnvGuard::lock();

        set_env_var(&env_guard, DISABLE_STDIN_FORWARDING_ENV, "1");

        Self {
            _env_guard: env_guard,
        }
    }
}

impl Drop for TestStdinForwardingGuard {
    fn drop(&mut self) {
        // SAFETY: `env_guard` keeps exclusive access to environment variables
        // until the knob is removed during drop.
        unsafe {
            std::env::remove_var(DISABLE_STDIN_FORWARDING_ENV);
        }
    }
}

/// Rstest fixture that provides a clean environment for tests.
///
/// This fixture clears all `PODBOT_*` environment variables and returns a guard
/// that holds exclusive access to the environment for the duration of the test.
///
/// # Example
///
/// ```ignore
/// use rstest::rstest;
/// use crate::test_utils::{clean_env, set_env_var, EnvGuard};
///
/// #[rstest]
/// fn my_test(clean_env: EnvGuard<'static>) {
///     set_env_var(&clean_env, "PODBOT_IMAGE", "test:latest");
///     // ... test code ...
/// }
/// ```
#[fixture]
pub fn clean_env() -> EnvGuard<'static> {
    clear_podbot_env()
}

#[cfg(test)]
mod tests {
    use bollard::exec::{CreateExecOptions, CreateExecResults, StartExecOptions, StartExecResults};
    use bollard::models::ExecInspectResponse;
    use mockall::mock;

    use super::*;
    use podbot::api::ExecMode;
    use podbot::engine::{CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture};

    mock! {
        #[derive(Debug)]
        TestExecClient {}

        impl ContainerExecClient for TestExecClient {
            fn create_exec(
                &self,
                container_id: &str,
                options: CreateExecOptions<String>,
            ) -> CreateExecFuture<'_>;
            fn start_exec(
                &self,
                exec_id: &str,
                options: Option<StartExecOptions>,
            ) -> StartExecFuture<'_>;
            fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
            fn resize_exec(
                &self,
                exec_id: &str,
                options: bollard::exec::ResizeExecOptions,
            ) -> ResizeExecFuture<'_>;
        }
    }

    #[test]
    fn set_env_var_updates_value_while_guard_is_held() {
        let guard = clear_podbot_env();
        set_env_var(&guard, "PODBOT_IMAGE", "test-image:latest");

        let image = std::env::var("PODBOT_IMAGE").expect("env var should be set");
        assert_eq!(image, "test-image:latest");
    }

    #[test]
    fn test_stdin_forwarding_guard_sets_and_clears_env_var() {
        let guard = TestStdinForwardingGuard::disable();

        let enabled_value = std::env::var(DISABLE_STDIN_FORWARDING_ENV)
            .expect("stdin forwarding knob should be enabled");
        assert_eq!(enabled_value, "1");

        drop(guard);

        let cleared_value = std::env::var(DISABLE_STDIN_FORWARDING_ENV);
        assert!(
            cleared_value.is_err(),
            "stdin forwarding knob should be removed"
        );
    }

    #[rstest]
    #[case(0, CommandOutcome::Success)]
    #[case(42, CommandOutcome::CommandExit { code: 42 })]
    fn exec_outcome_with_client_maps_exit_codes(
        #[case] exit_code: i64,
        #[case] expected: CommandOutcome,
    ) {
        let runtime = tokio::runtime::Runtime::new().expect("runtime should be created");
        let request = ExecRequest::new("sandbox", vec![String::from("echo"), String::from("ok")])
            .expect("request should be valid")
            .with_mode(ExecMode::Detached);
        let mut client = MockTestExecClient::new();

        client.expect_create_exec().times(1).returning(|_, _| {
            Box::pin(async {
                Ok(CreateExecResults {
                    id: String::from("test-exec-id"),
                })
            })
        });
        client
            .expect_start_exec()
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(StartExecResults::Detached) }));
        client.expect_inspect_exec().times(1).returning(move |_| {
            let inspect = ExecInspectResponse {
                running: Some(false),
                exit_code: Some(exit_code),
                ..ExecInspectResponse::default()
            };
            Box::pin(async move { Ok(inspect) })
        });
        client.expect_resize_exec().never();

        let result = exec_outcome_with_client(&client, runtime.handle(), &request)
            .expect("exec should succeed");

        assert_eq!(result, expected);
    }
}
