//! Shared host-stdio boundary helpers for exec session forwarding.

use std::pin::Pin;

use tokio::io::AsyncRead;

#[cfg(test)]
pub(super) fn default_host_stdin() -> Pin<Box<dyn AsyncRead + Send>> {
    Box::pin(tokio::io::empty())
}

#[cfg(not(test))]
pub(super) fn default_host_stdin() -> Pin<Box<dyn AsyncRead + Send>> {
    Box::pin(tokio::io::stdin())
}
