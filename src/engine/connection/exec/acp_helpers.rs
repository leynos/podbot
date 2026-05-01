//! Agentic Control Protocol (ACP) initialization frame rewriting for protocol-mode stdin
//! forwarding.
//!
//! This module reads the first newline-delimited frame from host stdin, rewrites the JSON
//! payload when it is an ACP `initialize` request that advertises `terminal` or `fs`
//! capabilities, and forwards the result to the container input. All other frames — including
//! malformed or non-ACP ones — are forwarded unchanged.

use std::io;
use std::pin::Pin;

use ortho_config::serde_json::{self, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt};

use super::STDIN_BUFFER_CAPACITY;

/// Maximum number of bytes buffered when reading the initial ACP frame.
///
/// Matches `STDIN_BUFFER_CAPACITY` so the first-frame buffer never exceeds the existing
/// bounded-buffering guarantee.
pub(super) const MAX_FIRST_FRAME_BYTES: usize = STDIN_BUFFER_CAPACITY;

/// The ACP method name that triggers capability masking.
pub(super) const ACP_INITIALIZE_METHOD: &str = "initialize";

/// The JSON field within `params` that advertises client capabilities.
pub(super) const ACP_CLIENT_CAPABILITIES_FIELD: &str = "clientCapabilities";

/// The `fs` capability family key removed from `clientCapabilities` during masking.
pub(super) const ACP_FILE_SYSTEM_CAPABILITY: &str = "fs";

/// The `terminal` capability family key removed from `clientCapabilities` during masking.
pub(super) const ACP_TERMINAL_CAPABILITY: &str = "terminal";

/// Reads and forwards the first newline-delimited frame from `buffered_stdin` to `input`.
///
/// When the frame is a valid ACP `initialize` request, `terminal` and `fs` capability entries
/// are removed from `params.clientCapabilities` before the bytes reach the container. If
/// `clientCapabilities` becomes empty after removal, the field itself is dropped.
///
/// Malformed JSON or frames that do not match the ACP `initialize` shape are forwarded without
/// modification. When no newline is found within [`MAX_FIRST_FRAME_BYTES`], the buffered bytes
/// are forwarded as-is so the caller can resume the normal byte-transparent copy loop.
pub(super) async fn forward_initial_acp_frame_async<HostStdin>(
    buffered_stdin: &mut tokio::io::BufReader<HostStdin>,
    input: &mut Pin<Box<dyn AsyncWrite + Send>>,
) -> io::Result<()>
where
    HostStdin: AsyncRead + Unpin,
{
    let mut first_frame = Vec::new();
    loop {
        if first_frame.len() == MAX_FIRST_FRAME_BYTES {
            tracing::debug!(
                bytes = MAX_FIRST_FRAME_BYTES,
                "initial ACP frame reached capacity limit; forwarding without masking",
            );
            input.write_all(&first_frame).await?;
            return Ok(());
        }

        let (bytes_to_consume, has_complete_frame) =
            read_next_bounded_frame_chunk(buffered_stdin, &mut first_frame).await?;
        if bytes_to_consume == 0 {
            tracing::debug!(
                bytes = first_frame.len(),
                "stdin closed before newline; forwarding partial initial frame without masking",
            );
            input.write_all(&first_frame).await?;
            return Ok(());
        }

        buffered_stdin.consume(bytes_to_consume);

        if has_complete_frame {
            return input
                .write_all(&mask_acp_initialize_frame(&first_frame))
                .await;
        }
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

/// Rewrites an ACP `initialize` frame by removing masked capability entries.
///
/// The function splits the frame into a JSON payload and its line ending, attempts to parse
/// the payload, and — when the message is a valid ACP `initialize` request — removes
/// `terminal` and `fs` from `params.clientCapabilities`. The original line ending bytes are
/// restored after re-serialisation.
///
/// Returns the original frame unchanged when JSON parsing fails, the message is not an ACP
/// `initialize` request, no masked capabilities are present, or re-serialisation fails.
pub(super) fn mask_acp_initialize_frame(frame: &[u8]) -> Vec<u8> {
    let (payload, line_ending) = split_frame_line_ending(frame);

    let Ok(mut message) = serde_json::from_slice(payload) else {
        tracing::warn!(
            bytes = frame.len(),
            "ACP initialize frame is not valid JSON; forwarding unchanged",
        );
        return frame.to_vec();
    };

    if !remove_masked_acp_capabilities(&mut message) {
        tracing::debug!("frame is not an ACP initialize request; forwarding unchanged");
        return frame.to_vec();
    }

    let Ok(mut serialized) = serde_json::to_vec(&message) else {
        tracing::warn!("failed to serialise masked ACP initialize frame; forwarding unchanged");
        return frame.to_vec();
    };

    tracing::debug!("ACP initialize frame masked: terminal and fs capabilities removed");
    serialized.extend_from_slice(line_ending);
    serialized
}

/// Splits a frame into its JSON payload and trailing line-ending bytes.
///
/// Returns `(payload, line_ending)` where `line_ending` is `b"\r\n"`, `b"\n"`, or `b""`.
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
