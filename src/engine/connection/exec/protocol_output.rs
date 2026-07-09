//! Container-output routing loops for protocol exec sessions.
//!
//! These helpers drain the container output stream and route each chunk to
//! host stdout or stderr, either directly (plain byte proxy) or through the
//! outbound ACP policy adapter when runtime enforcement is active. The stdout
//! purity contract documented in the parent module is upheld here: only
//! container stdout and console bytes ever reach host stdout.

use std::pin::Pin;

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::{Stream, StreamExt};
use tokio::io::{AsyncWrite, AsyncWriteExt};

use super::super::acp_runtime::OutboundPolicyAdapter;
use super::super::runtime_helpers::exec_failed;
use crate::error::PodbotError;

/// Borrowed IO handles threaded through the adapter-driven output loop.
pub(super) struct AdapterOutputIo<'a, HostStdout, HostStderr> {
    /// Outbound ACP policy adapter that inspects container stdout frames.
    pub(super) adapter: &'a mut OutboundPolicyAdapter,
    /// Host stdout writer used for container stdout and console output.
    pub(super) host_stdout: &'a mut HostStdout,
    /// Host stderr writer used for container stderr output.
    pub(super) host_stderr: &'a mut HostStderr,
}

/// Drain the container output stream through the outbound policy adapter.
pub(super) async fn run_output_loop_with_adapter<HostStdout, HostStderr>(
    container_id: &str,
    output: &mut Pin<Box<dyn Stream<Item = Result<LogOutput, BollardError>> + Send>>,
    io: &mut AdapterOutputIo<'_, HostStdout, HostStderr>,
) -> Result<(), PodbotError>
where
    HostStdout: AsyncWrite + Unpin,
    HostStderr: AsyncWrite + Unpin,
{
    while let Some(chunk_result) = output.next().await {
        let chunk = chunk_result
            .map_err(|error| exec_failed(container_id, format!("exec stream failed: {error}")))?;
        match chunk {
            LogOutput::StdOut { message } | LogOutput::Console { message } => {
                io.adapter
                    .handle_chunk(message.as_ref(), io.host_stdout)
                    .await
                    .map_err(|error| {
                        exec_failed(
                            container_id,
                            format!("failed writing stdout output: {error}"),
                        )
                    })?;
            }
            LogOutput::StdErr { message } => {
                write_output_chunk(container_id, io.host_stderr, message.as_ref(), "stderr")
                    .await?;
            }
            LogOutput::StdIn { .. } => {}
        }
    }
    Ok(())
}

/// Drain the container output stream, routing each chunk to host stdout or
/// stderr.
pub(super) async fn run_output_loop_async<HostStdout, HostStderr>(
    container_id: &str,
    output: &mut Pin<Box<dyn Stream<Item = Result<LogOutput, BollardError>> + Send>>,
    host_stdout: &mut HostStdout,
    host_stderr: &mut HostStderr,
) -> Result<(), PodbotError>
where
    HostStdout: AsyncWrite + Unpin,
    HostStderr: AsyncWrite + Unpin,
{
    while let Some(chunk_result) = output.next().await {
        let chunk = chunk_result
            .map_err(|error| exec_failed(container_id, format!("exec stream failed: {error}")))?;
        handle_log_output_chunk(container_id, chunk, host_stdout, host_stderr).await?;
    }

    Ok(())
}

/// Route a single container log-output chunk to the appropriate host stream.
async fn handle_log_output_chunk<HostStdout, HostStderr>(
    container_id: &str,
    chunk: LogOutput,
    host_stdout: &mut HostStdout,
    host_stderr: &mut HostStderr,
) -> Result<(), PodbotError>
where
    HostStdout: AsyncWrite + Unpin,
    HostStderr: AsyncWrite + Unpin,
{
    match chunk {
        LogOutput::StdOut { message } | LogOutput::Console { message } => {
            write_output_chunk(container_id, host_stdout, message.as_ref(), "stdout").await
        }
        LogOutput::StdErr { message } => {
            write_output_chunk(container_id, host_stderr, message.as_ref(), "stderr").await
        }
        LogOutput::StdIn { .. } => Ok(()),
    }
}

/// Write and flush a byte slice to `writer`, mapping I/O failures to
/// `PodbotError`.
async fn write_output_chunk<Writer>(
    container_id: &str,
    writer: &mut Writer,
    bytes: &[u8],
    stream_name: &str,
) -> Result<(), PodbotError>
where
    Writer: AsyncWrite + Unpin,
{
    writer.write_all(bytes).await.map_err(|error| {
        exec_failed(
            container_id,
            format!("failed writing {stream_name} output: {error}"),
        )
    })?;
    writer.flush().await.map_err(|error| {
        exec_failed(
            container_id,
            format!("failed flushing {stream_name} output: {error}"),
        )
    })?;
    Ok(())
}
