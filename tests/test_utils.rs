//! Shared test utilities for integration tests.
//!
//! This module provides common helpers used across integration tests, particularly
//! for environment variable manipulation which requires careful synchronization.

use std::sync::{Mutex, MutexGuard};

use podbot::config::env_var_names;
use rstest::fixture;

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
/// Uses [`env_var_names()`] from the loader to stay in sync with the actual
/// environment variable mappings. Also clears `PODBOT_CONFIG_PATH` which is
/// handled separately by the config discovery mechanism.
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
