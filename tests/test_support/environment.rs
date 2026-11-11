//! Scoped ambient-environment boundary for synchronous integration harnesses.
//!
//! Only third-party startup and test-only stdin selection use this boundary.
//! Prefer injected configuration elsewhere; finish every async operation with
//! `block_on` inside the closure so its environment cannot escape the scope.

/// Run an operation with one temporary variable and restore its prior value.
///
/// `temp_env` owns the shared reentrant mutex and unwind-safe restoration.
/// Callers must complete spawned environment readers before returning and must
/// not return an environment-dependent future to be polled after restoration.
/// This does not synchronize arbitrary
/// environment readers outside this test boundary.
///
/// # Example
///
/// ```ignore
/// let result = with_test_environment("DOCKER_HOST", endpoint, || {
///     runtime.block_on(start_container())
/// });
/// ```
pub fn with_test_environment<F, R>(key: &str, value: &str, operation: F) -> R
where
    F: FnOnce() -> R,
{
    temp_env::with_var(key, Some(value), operation)
}

#[cfg(test)]
mod tests {
    //! Regression tests for restoration and synchronous async lifetimes.

    use std::{env, panic, time::Duration};

    use super::with_test_environment;

    /// Variable reserved for restoration regression cases.
    const KEY: &str = "PODBOT_SCOPED_ENVIRONMENT_REGRESSION";

    #[test]
    fn nested_scopes_restore_an_existing_value_after_error() {
        temp_env::with_var(KEY, Some("original"), || {
            let (value, result) = with_test_environment(KEY, "temporary", || {
                (env::var(KEY), Err::<(), &str>("operation failed"))
            });
            assert_eq!(
                value.expect("temporary value should exist"),
                "temporary",
                "scope should install its value"
            );
            assert_eq!(
                result,
                Err("operation failed"),
                "operation error should propagate unchanged"
            );
            let restored = env::var(KEY).expect("original value should be restored");
            assert_eq!(restored, "original", "scope should restore the prior value");
        });
    }

    #[test]
    fn success_restores_an_absent_variable() {
        temp_env::with_var_unset(KEY, || {
            let value = with_test_environment(KEY, "temporary", || env::var(KEY));
            assert_eq!(
                value.expect("temporary value should exist"),
                "temporary",
                "scope should install its value"
            );
            assert!(
                env::var_os(KEY).is_none(),
                "absent value should be restored"
            );
        });
    }

    /// Observe one worker's value while its scoped environment lock is held.
    ///
    /// Only the concurrency regression uses this helper; assertions stay in the
    /// test body after joining both workers.
    fn read_concurrent_value(value: &str) -> Result<String, env::VarError> {
        /// Separate variable for the two-worker isolation case.
        const CONCURRENT_KEY: &str = "PODBOT_SCOPED_ENVIRONMENT_CONCURRENCY";
        with_test_environment(CONCURRENT_KEY, value, || {
            std::thread::yield_now();
            env::var(CONCURRENT_KEY)
        })
    }

    #[test]
    fn concurrent_operations_keep_their_own_values() {
        let handles = ["first", "second"]
            .map(|value| std::thread::spawn(move || (value, read_concurrent_value(value))));
        for handle in handles {
            let (expected, observed) = handle.join().expect("environment worker should finish");
            assert_eq!(
                observed.expect("worker value should exist"),
                expected,
                "concurrent scopes should remain isolated"
            );
        }
    }

    #[test]
    fn panic_restores_an_absent_variable() {
        temp_env::with_var_unset(KEY, || {
            let result = panic::catch_unwind(|| {
                with_test_environment(KEY, "temporary", || panic!("operation panicked"));
            });
            assert!(result.is_err(), "operation panic should propagate");
            assert!(
                env::var_os(KEY).is_none(),
                "absent value should be restored"
            );
        });
    }

    /// Complete the runtime regression's observation before its caller restores KEY.
    ///
    /// Call only inside the regression's scoped environment closure. Keeping
    /// `block_on` here polls and cancels the future before the scope returns.
    fn observe_yield_and_cancellation(
        runtime: &tokio::runtime::Runtime,
    ) -> (Result<String, env::VarError>, bool) {
        runtime.block_on(async {
            tokio::task::yield_now().await;
            let value = env::var(KEY);
            let timed_out =
                tokio::time::timeout(Duration::from_millis(1), std::future::pending::<()>())
                    .await
                    .is_err();
            (value, timed_out)
        })
    }

    #[test]
    fn runtime_operation_sees_value_after_yield_and_restores_on_cancellation() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("current-thread runtime should be created");
        temp_env::with_var(KEY, Some("original"), || {
            let (value, timed_out) = with_test_environment(KEY, "temporary", || {
                observe_yield_and_cancellation(&runtime)
            });
            assert_eq!(
                value.expect("value should survive suspension"),
                "temporary",
                "value should remain active after an async yield"
            );
            assert!(
                timed_out,
                "pending operation should be cancelled by timeout"
            );
            let restored = env::var(KEY).expect("original value should be restored");
            assert_eq!(restored, "original", "scope should restore the prior value");
        });
    }
}
