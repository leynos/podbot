//! ACP initialization frame rewriting for protocol-mode stdin forwarding.
//!
//! ## Concurrency model
//!
//! Each protocol session runs its own independent forwarding task on a single
//! Tokio task. The `BufReader` wrapping host stdin is owned exclusively by that
//! task and is never shared across tasks or threads. All state is stack-local or
//! moved into the task at spawn time, so there are no shared-mutable references
//! and no synchronization primitives are required. If the forwarding task is
//! cancelled at an `await` point the `BufReader` and container input writer are
//! both dropped, releasing the underlying pipe handles cleanly.

use std::io;
use std::pin::Pin;

use ortho_config::serde_json::{self, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt};

use super::STDIN_BUFFER_CAPACITY;

/// Upper bound on the number of bytes buffered while searching for the first
/// ACP frame. Frames that exceed this limit are forwarded as-is without
/// attempting JSON rewriting.
pub(super) const MAX_FIRST_FRAME_BYTES: usize = STDIN_BUFFER_CAPACITY;
/// The JSON-RPC `method` value that identifies an ACP initialization request.
pub(super) const ACP_INITIALIZE_METHOD: &str = "initialize";
/// The `params` field that carries the client's advertised ACP capabilities.
pub(super) const ACP_CLIENT_CAPABILITIES_FIELD: &str = "clientCapabilities";
/// The `clientCapabilities` key for filesystem capability advertisements,
/// which are masked before the frame is forwarded to the container.
pub(super) const ACP_FILE_SYSTEM_CAPABILITY: &str = "fs";
/// The `clientCapabilities` key for terminal capability advertisements,
/// which are masked before the frame is forwarded to the container.
pub(super) const ACP_TERMINAL_CAPABILITY: &str = "terminal";

enum InitialFrameAction {
    Continue,
    ForwardMasked,
    ForwardUnchanged(ForwardUnchangedReason),
}

#[derive(Clone, Copy)]
enum ForwardUnchangedReason {
    ExceededMaximumSize,
    EofBeforeNewline,
}

/// Reads the first newline-delimited ACP frame from `buffered_stdin`, rewrites
/// it by removing masked capabilities, and writes the result to `input`.
///
/// If the frame exceeds [`MAX_FIRST_FRAME_BYTES`] before a newline is found,
/// or if EOF is reached first, the buffered bytes are forwarded unchanged.
pub(super) async fn forward_initial_acp_frame_async<HostStdin>(
    buffered_stdin: &mut tokio::io::BufReader<HostStdin>,
    input: &mut Pin<Box<dyn AsyncWrite + Send>>,
) -> io::Result<()>
where
    HostStdin: AsyncRead + Unpin,
{
    let bytes = read_and_mask_initial_acp_frame(buffered_stdin).await?;
    input.write_all(&bytes).await
}

/// Reads the first newline-delimited ACP frame from `buffered_stdin` and
/// returns the bytes that the protocol session should forward to the
/// container, applying capability masking when the frame is a recognized
/// `initialize` request.
///
/// If the frame exceeds [`MAX_FIRST_FRAME_BYTES`] before a newline is found,
/// or if EOF is reached first, the buffered bytes are returned unchanged so
/// the caller can forward them verbatim.
pub(super) async fn read_and_mask_initial_acp_frame<HostStdin>(
    buffered_stdin: &mut tokio::io::BufReader<HostStdin>,
) -> io::Result<Vec<u8>>
where
    HostStdin: AsyncRead + Unpin,
{
    let mut first_frame = Vec::new();
    loop {
        match next_initial_frame_action(buffered_stdin, &mut first_frame).await? {
            InitialFrameAction::Continue => {}
            InitialFrameAction::ForwardUnchanged(reason) => {
                log_unmodified_forwarding(reason, first_frame.len());
                return Ok(first_frame);
            }
            InitialFrameAction::ForwardMasked => {
                let masked = mask_acp_initialize_frame(&first_frame);
                log_masked_frame_forwarded(&masked, &first_frame);
                return Ok(masked);
            }
        }
    }
}

async fn next_initial_frame_action<HostStdin>(
    buffered_stdin: &mut tokio::io::BufReader<HostStdin>,
    first_frame: &mut Vec<u8>,
) -> io::Result<InitialFrameAction>
where
    HostStdin: AsyncRead + Unpin,
{
    if first_frame.len() == MAX_FIRST_FRAME_BYTES {
        return Ok(InitialFrameAction::ForwardUnchanged(
            ForwardUnchangedReason::ExceededMaximumSize,
        ));
    }

    let (bytes_to_consume, has_complete_frame) =
        read_next_bounded_frame_chunk(buffered_stdin, first_frame).await?;
    if bytes_to_consume == 0 {
        return Ok(InitialFrameAction::ForwardUnchanged(
            ForwardUnchangedReason::EofBeforeNewline,
        ));
    }

    buffered_stdin.consume(bytes_to_consume);
    Ok(if has_complete_frame {
        InitialFrameAction::ForwardMasked
    } else {
        InitialFrameAction::Continue
    })
}

fn log_unmodified_forwarding(reason: ForwardUnchangedReason, bytes: usize) {
    match reason {
        ForwardUnchangedReason::ExceededMaximumSize => log_exceeded_maximum_size(bytes),
        ForwardUnchangedReason::EofBeforeNewline => log_eof_before_newline(bytes),
    }
}

fn log_exceeded_maximum_size(bytes: usize) {
    tracing::debug!(
        bytes,
        "ACP first frame exceeded maximum size; forwarding without rewrite"
    );
}

fn log_eof_before_newline(bytes: usize) {
    tracing::debug!(
        bytes,
        "ACP stdin reached EOF before newline; forwarding without rewrite"
    );
}

fn log_masked_frame_forwarded(masked_frame: &[u8], first_frame: &[u8]) {
    if masked_frame != first_frame {
        tracing::debug!("ACP initialize frame masked and forwarded");
    }
}

async fn read_next_bounded_frame_chunk<HostStdin>(
    buffered_stdin: &mut tokio::io::BufReader<HostStdin>,
    first_frame: &mut Vec<u8>,
) -> io::Result<(usize, bool)>
where
    HostStdin: AsyncRead + Unpin,
{
    let buffered = buffered_stdin.fill_buf().await?;
    if buffered.is_empty() {
        return Ok((0, false));
    }

    let remaining_capacity = MAX_FIRST_FRAME_BYTES.saturating_sub(first_frame.len());
    let bytes_to_scan = buffered.len().min(remaining_capacity);
    let newline_offset = buffered
        .get(..bytes_to_scan)
        .and_then(|bytes| bytes.iter().position(|byte| *byte == b'\n'));
    let bytes_to_consume = newline_offset.map_or(bytes_to_scan, |offset| offset + 1);
    let Some(bytes) = buffered.get(..bytes_to_consume) else {
        return Err(io::Error::other(
            "buffered stdin slice exceeded available input",
        ));
    };
    first_frame.extend_from_slice(bytes);
    Ok((bytes_to_consume, newline_offset.is_some()))
}

/// Rewrites an ACP `initialize` frame by removing `terminal` and `fs` from
/// `params.clientCapabilities`, then re-serializes the JSON preserving the
/// original line ending. Returns the original frame unchanged on any parse or
/// serialization failure, or if no capabilities were removed.
#[expect(
    clippy::cognitive_complexity,
    reason = "inline parse and serialize fallback tracing keeps this review-required flow local"
)]
pub(super) fn mask_acp_initialize_frame(frame: &[u8]) -> Vec<u8> {
    let (payload, line_ending) = split_frame_line_ending(frame);

    let mut message: Value = match serde_json::from_slice(payload) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(
                error = %e,
                "ACP frame JSON deserialization failed; forwarding unchanged"
            );
            return frame.to_vec();
        }
    };

    if !remove_masked_acp_capabilities(&mut message) {
        tracing::debug!("ACP frame is not a maskable initialize request; forwarding unchanged");
        return frame.to_vec();
    }

    let mut serialized = match serde_json::to_vec(&message) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(
                error = %e,
                "ACP frame JSON serialization failed after masking; forwarding unchanged"
            );
            return frame.to_vec();
        }
    };
    serialized.extend_from_slice(line_ending);
    serialized
}

/// Splits a newline-delimited frame into its JSON payload and trailing line
/// ending bytes (`\n` or `\r\n`). Returns the original slice as the payload
/// with an empty line-ending slice when no recognised line ending is present.
pub(super) fn split_frame_line_ending(frame: &[u8]) -> (&[u8], &[u8]) {
    if let Some(stripped) = frame.strip_suffix(b"\r\n") {
        return (stripped, b"\r\n");
    }

    if let Some(stripped) = frame.strip_suffix(b"\n") {
        return (stripped, b"\n");
    }

    (frame, b"")
}

fn remove_masked_acp_capabilities(message: &mut Value) -> bool {
    if message.get("method").and_then(Value::as_str) != Some(ACP_INITIALIZE_METHOD) {
        return false;
    }

    let Some(params) = message.get_mut("params").and_then(Value::as_object_mut) else {
        return false;
    };

    let Some(client_capabilities) = params
        .get_mut(ACP_CLIENT_CAPABILITIES_FIELD)
        .and_then(Value::as_object_mut)
    else {
        return false;
    };

    let removed_terminal = client_capabilities
        .remove(ACP_TERMINAL_CAPABILITY)
        .is_some();
    let removed_fs = client_capabilities
        .remove(ACP_FILE_SYSTEM_CAPABILITY)
        .is_some();
    if !removed_terminal && !removed_fs {
        return false;
    }

    if client_capabilities.is_empty() {
        params.remove(ACP_CLIENT_CAPABILITIES_FIELD);
    }

    true
}
