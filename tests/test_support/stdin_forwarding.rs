//! Scoped stdin selection for the interactive-exec and orchestration BDD suites.
//!
//! Only those two integration-test roots include this module; generic test
//! utilities must not pull its environment mutation into unrelated suites.

/// Shared scoped environment boundary for the complete synchronous operation.
#[path = "environment.rs"]
mod environment;

/// Run a complete synchronous exec operation without host stdin forwarding.
///
/// Any `block_on` call belongs inside `operation`; returning an
/// unpolled future would restore the environment before the operation runs.
/// Previous values are restored on success, error, or panic.
///
/// # Example
///
/// ```ignore
/// let outcome = with_stdin_forwarding_disabled(|| runtime.block_on(exec()));
/// ```
pub fn with_stdin_forwarding_disabled<F, R>(operation: F) -> R
where
    F: FnOnce() -> R,
{
    environment::with_test_environment("PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS", "1", operation)
}
