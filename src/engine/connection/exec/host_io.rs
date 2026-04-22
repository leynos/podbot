//! Shared host-stdio boundary helpers for exec session forwarding.
//!
//! Unit tests always receive `tokio::io::empty()` so they never consume the
//! runner's stdin. In non-test builds, integration tests can force the same
//! empty reader by setting `PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS=1`;
//! otherwise Podbot forwards the real host stdin.

use std::pin::Pin;

use tokio::io::AsyncRead;

#[cfg(not(test))]
const DISABLE_STDIN_FORWARDING_ENV: &str = "PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS";

/// Returns `false` in test builds; stdin forwarding is always treated as
/// enabled at this layer so that `default_host_stdin()` returns the
/// appropriate empty reader via its own `#[cfg(test)]` variant.
#[cfg(test)]
pub(super) const fn stdin_forwarding_disabled_for_tests() -> bool {
    false
}

/// Returns `true` when the `PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS`
/// environment variable is set to `"1"`, allowing integration tests to
/// suppress real stdin forwarding without a rebuild.
#[cfg(not(test))]
pub(super) fn stdin_forwarding_disabled_for_tests() -> bool {
    std::env::var(DISABLE_STDIN_FORWARDING_ENV).is_ok_and(|value| value == "1")
}

/// Returns a deterministic empty stdin reader for unit tests.
///
/// Ensures unit tests never block on or consume the test runner's real stdin.
#[cfg(test)]
pub(super) fn default_host_stdin() -> Pin<Box<dyn AsyncRead + Send>> {
    Box::pin(tokio::io::empty())
}

/// Returns host stdin unless stdin forwarding is explicitly disabled.
///
/// When `PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS=1` is set, returns
/// `tokio::io::empty()` instead of the real `tokio::io::stdin()`.
#[cfg(not(test))]
pub(super) fn default_host_stdin() -> Pin<Box<dyn AsyncRead + Send>> {
    if stdin_forwarding_disabled_for_tests() {
        return Box::pin(tokio::io::empty());
    }
    Box::pin(tokio::io::stdin())
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncReadExt;

    use super::*;

    #[tokio::test]
    async fn default_host_stdin_returns_empty_reader_in_tests() {
        let mut stdin = default_host_stdin();
        let mut bytes = Vec::new();
        let bytes_read = stdin
            .read_to_end(&mut bytes)
            .await
            .expect("test stdin reader should be readable");

        assert_eq!(bytes_read, 0, "test stdin should be empty");
        assert!(bytes.is_empty(), "test stdin should not yield any bytes");
    }
}
