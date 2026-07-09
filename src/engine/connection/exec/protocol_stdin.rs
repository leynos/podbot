//! Host-stdin forwarding and shutdown settlement for protocol exec sessions.
//!
//! These helpers own the stdin side of the protocol byte proxy: copying host
//! stdin into the container exec input (optionally rewriting the first ACP
//! `initialize` frame), pumping stdin chunks into the container-stdin sink
//! channel under runtime enforcement, and settling or aborting the forwarding
//! task during session shutdown.

use std::io;
use std::pin::Pin;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::super::acp_helpers;
use super::super::acp_runtime::WriteCmd;
use super::super::host_io::stdin_forwarding_disabled_for_tests;
use super::super::runtime_helpers::exec_failed;
use super::{ProtocolSessionOptions, STDIN_BUFFER_CAPACITY};
use crate::error::PodbotError;

/// Allow a short grace period for EOF- and flush-driven completion paths to
/// finish before treating stdin forwarding as stalled. `50ms` matches typical
/// local pipe and socket flush timings in this code path without adding a
/// noticeable shutdown delay; if future benchmarks or transport changes show
/// different behaviour, adjust `STDIN_SETTLE_TIMEOUT` and extend the proxy
/// tests that exercise EOF and non-EOF stdin shutdown cases.
const STDIN_SETTLE_TIMEOUT: Duration = Duration::from_millis(50);

/// Reads the first newline-delimited ACP frame from `buffered_stdin`,
/// applies capability masking, and forwards the resulting bytes to
/// `sender` as a [`WriteCmd::Forward`].
///
/// Returns `Ok(true)` when the caller should continue pumping host
/// stdin (either because the masker produced no bytes to forward, or
/// because the send to the sink succeeded). Returns `Ok(false)` when
/// the sink channel has closed and the caller should return cleanly.
async fn send_masked_initialize_frame<R>(
    buffered_stdin: &mut tokio::io::BufReader<R>,
    sender: &tokio::sync::mpsc::Sender<WriteCmd>,
) -> io::Result<bool>
where
    R: AsyncRead + Unpin,
{
    let bytes = acp_helpers::read_and_mask_initial_acp_frame(buffered_stdin).await?;
    if bytes.is_empty() {
        return Ok(true);
    }
    Ok(sender.send(WriteCmd::Forward(bytes)).await.is_ok())
}

/// Pumps the remainder of host stdin into the container-stdin sink as
/// a sequence of [`WriteCmd::Forward`] chunks bounded by
/// [`STDIN_BUFFER_CAPACITY`]. Stops on EOF or when the sink channel
/// closes.
async fn pump_raw_frames<R>(
    buffered_stdin: &mut tokio::io::BufReader<R>,
    sender: &tokio::sync::mpsc::Sender<WriteCmd>,
) -> io::Result<()>
where
    R: AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut buf = vec![0u8; STDIN_BUFFER_CAPACITY];
    loop {
        let bytes_read = buffered_stdin.read(&mut buf).await?;
        if bytes_read == 0 {
            break;
        }
        let chunk = buf
            .get(..bytes_read)
            .map(<[u8]>::to_vec)
            .unwrap_or_default();
        if sender.send(WriteCmd::Forward(chunk)).await.is_err() {
            break;
        }
    }
    Ok(())
}

/// Copy host stdin into the container-stdin sink channel, optionally masking
/// the first ACP `initialize` frame before the raw copy begins.
pub(super) async fn forward_host_stdin_to_channel<HostStdin>(
    host_stdin: HostStdin,
    sender: tokio::sync::mpsc::Sender<WriteCmd>,
    rewrite_acp_initialize: bool,
) -> io::Result<()>
where
    HostStdin: AsyncRead + Unpin,
{
    let mut buffered_stdin = tokio::io::BufReader::with_capacity(STDIN_BUFFER_CAPACITY, host_stdin);

    if rewrite_acp_initialize && !send_masked_initialize_frame(&mut buffered_stdin, &sender).await?
    {
        return Ok(());
    }

    pump_raw_frames(&mut buffered_stdin, &sender).await
}

/// Wait for the stdin forwarding task to complete within a short grace
/// period.
pub(super) async fn settle_stdin_forwarding_task(
    container_id: &str,
    mut stdin_task: JoinHandle<io::Result<()>>,
    options: ProtocolSessionOptions,
) -> Result<(), PodbotError> {
    let Ok(join_result) = timeout(STDIN_SETTLE_TIMEOUT, &mut stdin_task).await else {
        // The container output path has already completed, so stdin can be
        // cancelled instead of waiting indefinitely on a live host reader. A
        // timeout here still indicates that stdin forwarding did not complete
        // cleanly before shutdown, so protocol mode must surface that failure
        // instead of reporting success with potentially truncated input.
        abort_stdin_forwarding_task(stdin_task);
        if options.disable_stdin_forwarding || stdin_forwarding_disabled_for_tests() {
            return Ok(());
        }
        return Err(exec_failed(
            container_id,
            "stdin forwarding did not complete before protocol session shutdown",
        ));
    };

    classify_stdin_forwarding_task_result(container_id, join_result)
}

/// Map a join result from the stdin forwarding task to a `PodbotError`.
fn classify_stdin_forwarding_task_result(
    container_id: &str,
    join_result: Result<io::Result<()>, tokio::task::JoinError>,
) -> Result<(), PodbotError> {
    match join_result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(exec_failed(
            container_id,
            format!("failed forwarding stdin to exec input: {error}"),
        )),
        Err(error) if error.is_cancelled() => Ok(()),
        Err(error) => Err(exec_failed(
            container_id,
            format!("stdin forwarding task failed: {error}"),
        )),
    }
}

/// Abort and drop the stdin forwarding task without awaiting it.
fn abort_stdin_forwarding_task(stdin_task: JoinHandle<io::Result<()>>) {
    if !stdin_task.is_finished() {
        stdin_task.abort();
        // Avoid awaiting the aborted task here because host stdin may be
        // blocked in a non-cancellable read. Dropping the handle mirrors the
        // attached-session shutdown path and keeps teardown bounded.
        drop(stdin_task);
    }
}

/// Copy host stdin to the container exec input, optionally rewriting the
/// first ACP `initialize` frame before the raw copy begins.
pub(super) async fn forward_host_stdin_to_exec_async<HostStdin>(
    host_stdin: HostStdin,
    mut input: Pin<Box<dyn AsyncWrite + Send>>,
    rewrite_acp_initialize: bool,
) -> io::Result<()>
where
    HostStdin: AsyncRead + Unpin,
{
    let mut buffered_stdin = tokio::io::BufReader::with_capacity(STDIN_BUFFER_CAPACITY, host_stdin);

    if rewrite_acp_initialize {
        acp_helpers::forward_initial_acp_frame_async(&mut buffered_stdin, &mut input).await?;
    }
    tokio::io::copy(&mut buffered_stdin, &mut input).await?;

    input.flush().await?;
    input.shutdown().await
}
