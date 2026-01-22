//! Shared test utilities for integration tests.
//!
//! This module provides common helpers used across integration tests, particularly
//! for environment variable manipulation which requires careful synchronization.

use std::sync::{Mutex, MutexGuard};

use podbot::config::env_var_names;

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

/// Sets an environment variable with exclusive access.
///
/// This helper acquires environment mutex, sets the environment variable,
/// and returns a guard that holds the lock. The caller must ensure the
/// guard's lifetime encompasses the duration where the environment variable
/// should be set.
///
/// # Safety
///
/// The unsafe block is required because `std::env::set_var` is unsafe in
/// Rust 2024 edition. Safety is ensured by the mutex guard which guarantees
/// exclusive access to environment variables.
///
/// # Example
///
/// ```ignore
/// #[test]
/// #[serial]
/// fn my_test() {
///     test_utils::set_env_var("FOO", "bar");
///     // Environment variable is now set
/// } // Guard dropped here, lock released
/// ```
#[allow(
    clippy::allow_attributes,
    dead_code,
    reason = "Utility function kept for future use; needed here for the dead_code suppress"
)]
pub fn set_env_var(key: &str, value: &str) {
    let _guard = EnvGuard::lock();
    // SAFETY: Mutex guard ensures exclusive access to environment variables.
    unsafe {
        std::env::set_var(key, value);
    }
}
