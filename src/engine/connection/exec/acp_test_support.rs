//! Shared test doubles and frame builders for the ACP test modules.
//!
//! Consolidates the recording writer used to capture host or container
//! output in tests, together with the newline-terminated JSON-RPC frame
//! builder, so the individual ACP test modules do not duplicate them.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};

use serde_json::Value;
use tokio::io::AsyncWrite;

/// Recording writer that captures every byte written to it and tracks
/// whether `poll_shutdown` was observed.
///
/// Clones share the same underlying buffers, so a test can clone the
/// writer before moving it into the code under test and query the clone
/// afterwards.
#[derive(Clone, Default)]
pub(super) struct RecordingWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
    shutdown_called: Arc<Mutex<bool>>,
}

impl RecordingWriter {
    /// Create a fresh recorder with empty buffers.
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Return a copy of the bytes captured so far.
    pub(super) fn snapshot(&self) -> Vec<u8> {
        self.bytes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Return `true` when `poll_shutdown` has been called on any clone.
    pub(super) fn shutdown_observed(&self) -> bool {
        *self
            .shutdown_called
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl AsyncWrite for RecordingWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.bytes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        *self
            .shutdown_called
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = true;
        Poll::Ready(Ok(()))
    }
}

/// Build a serialised JSON-RPC 2.0 frame terminated by `line_ending`.
///
/// Pass `id = Some(…)` for requests and `id = None` for notifications.
pub(super) fn jsonrpc_frame(
    id: Option<&Value>,
    method: &str,
    line_ending: &[u8],
) -> Result<Vec<u8>, serde_json::Error> {
    let payload = id.map_or_else(
        || {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": {},
            })
        },
        |request_id| {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": method,
                "params": {},
            })
        },
    );
    let mut bytes = serde_json::to_vec(&payload)?;
    bytes.extend_from_slice(line_ending);
    Ok(bytes)
}
