//! Shared host-stdio boundary helpers for exec session forwarding.

use std::pin::Pin;

use tokio::io::AsyncRead;

#[cfg(not(test))]
pub(super) const DISABLE_STDIN_FORWARDING_ENV: &str = "PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS";

#[cfg(test)]
pub(super) fn default_host_stdin() -> Pin<Box<dyn AsyncRead + Send>> {
    Box::pin(tokio::io::empty())
}

#[cfg(not(test))]
pub(super) fn default_host_stdin() -> Pin<Box<dyn AsyncRead + Send>> {
    if std::env::var_os(DISABLE_STDIN_FORWARDING_ENV).is_some() {
        Box::pin(tokio::io::empty())
    } else {
        Box::pin(tokio::io::stdin())
    }
}
