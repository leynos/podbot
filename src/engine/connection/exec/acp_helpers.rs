//! ACP initialization frame rewriting for protocol-mode stdin forwarding.

use std::io;
use std::pin::Pin;

use ortho_config::serde_json::{self, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt};

use super::STDIN_BUFFER_CAPACITY;

pub(super) const MAX_FIRST_FRAME_BYTES: usize = STDIN_BUFFER_CAPACITY;
pub(super) const ACP_INITIALIZE_METHOD: &str = "initialize";
pub(super) const ACP_CLIENT_CAPABILITIES_FIELD: &str = "clientCapabilities";
pub(super) const ACP_FILE_SYSTEM_CAPABILITY: &str = "fs";
pub(super) const ACP_TERMINAL_CAPABILITY: &str = "terminal";

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
            input.write_all(&first_frame).await?;
            return Ok(());
        }

        let (bytes_to_consume, has_complete_frame) =
            read_next_bounded_frame_chunk(buffered_stdin, &mut first_frame).await?;
        if bytes_to_consume == 0 {
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

pub(super) fn mask_acp_initialize_frame(frame: &[u8]) -> Vec<u8> {
    let (payload, line_ending) = split_frame_line_ending(frame);

    let Ok(mut message) = serde_json::from_slice(payload) else {
        return frame.to_vec();
    };

    if !remove_masked_acp_capabilities(&mut message) {
        return frame.to_vec();
    }

    let Ok(mut serialized) = serde_json::to_vec(&message) else {
        return frame.to_vec();
    };
    serialized.extend_from_slice(line_ending);
    serialized
}

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
