//! Attached exec-session stream and IO forwarding helpers.

use std::io;
use std::pin::Pin;
use std::time::Duration;

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::{Stream, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::sleep;

use super::terminal::{
    TerminalSizeProvider, maybe_sigwinch_listener, resize_exec_to_current_terminal_async,
    wait_for_sigwinch,
};
use super::{ContainerExecClient, EXEC_INSPECT_POLL_INTERVAL_MS, ExecRequest, exec_failed};
use crate::error::PodbotError;

#[expect(
    clippy::too_many_arguments,
    reason = "attached exec loop needs explicit state to keep helpers simple"
)]
#[expect(
    clippy::cognitive_complexity,
    reason = "attached flow combines stream forwarding and resize handling"
)]
#[expect(
    clippy::integer_division_remainder_used,
    reason = "false positive triggered inside tokio::select! expansion"
)]
pub(super) async fn run_attached_session_async<C: ContainerExecClient, P: TerminalSizeProvider>(
    client: &C,
    request: &ExecRequest,
    exec_id: &str,
    mut output: Pin<Box<dyn Stream<Item = Result<LogOutput, BollardError>> + Send>>,
    input: Pin<Box<dyn AsyncWrite + Send>>,
    size_provider: &P,
) -> Result<(), PodbotError> {
    let stdin_task = tokio::spawn(async move { forward_stdin_to_exec_async(input).await });
    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();

    #[cfg(unix)]
    let mut sigwinch = maybe_sigwinch_listener(request)?;

    if request.tty() {
        resize_exec_to_current_terminal_async(
            client,
            request.container_id(),
            exec_id,
            size_provider,
        )
        .await?;
    }

    loop {
        #[cfg(unix)]
        {
            tokio::select! {
                maybe_chunk = output.next() => {
                    if !write_exec_output_chunk(request.container_id(), maybe_chunk, &mut stdout, &mut stderr).await? {
                        break;
                    }
                }
                () = wait_for_sigwinch(&mut sigwinch), if sigwinch.is_some() => {
                    resize_exec_to_current_terminal_async(client, request.container_id(), exec_id, size_provider).await?;
                }
            }
        }
        #[cfg(not(unix))]
        {
            let maybe_chunk = output.next().await;
            if !write_exec_output_chunk(
                request.container_id(),
                maybe_chunk,
                &mut stdout,
                &mut stderr,
            )
            .await?
            {
                break;
            }
        }
    }

    stdin_task.abort();
    if let Err(join_error) = stdin_task.await {
        if !join_error.is_cancelled() {
            return Err(exec_failed(
                request.container_id(),
                format!("failed to join stdin forwarding task: {join_error}"),
            ));
        }
    }

    Ok(())
}

pub(super) async fn wait_for_exit_code_async<C: ContainerExecClient>(
    client: &C,
    container_id: &str,
    exec_id: &str,
) -> Result<i64, PodbotError> {
    loop {
        let inspect = client
            .inspect_exec(exec_id)
            .await
            .map_err(|error| exec_failed(container_id, format!("inspect exec failed: {error}")))?;

        if inspect.running.unwrap_or(false) {
            sleep(Duration::from_millis(EXEC_INSPECT_POLL_INTERVAL_MS)).await;
            continue;
        }

        if let Some(exit_code) = inspect.exit_code {
            return Ok(exit_code);
        }

        return Err(exec_failed(
            container_id,
            format!("exec session '{exec_id}' completed without an exit code"),
        ));
    }
}

async fn write_exec_output_chunk(
    container_id: &str,
    maybe_chunk: Option<Result<LogOutput, BollardError>>,
    stdout: &mut tokio::io::Stdout,
    stderr: &mut tokio::io::Stderr,
) -> Result<bool, PodbotError> {
    let Some(chunk_result) = maybe_chunk else {
        return Ok(false);
    };
    let chunk = chunk_result
        .map_err(|error| exec_failed(container_id, format!("exec stream failed: {error}")))?;

    match chunk {
        LogOutput::StdErr { message } => {
            stderr.write_all(message.as_ref()).await.map_err(|error| {
                exec_failed(
                    container_id,
                    format!("failed writing stderr output: {error}"),
                )
            })?;
            stderr.flush().await.map_err(|error| {
                exec_failed(
                    container_id,
                    format!("failed flushing stderr output: {error}"),
                )
            })?;
        }
        LogOutput::StdOut { message }
        | LogOutput::Console { message }
        | LogOutput::StdIn { message } => {
            stdout.write_all(message.as_ref()).await.map_err(|error| {
                exec_failed(
                    container_id,
                    format!("failed writing stdout output: {error}"),
                )
            })?;
            stdout.flush().await.map_err(|error| {
                exec_failed(
                    container_id,
                    format!("failed flushing stdout output: {error}"),
                )
            })?;
        }
    }

    Ok(true)
}

async fn forward_stdin_to_exec_async(mut input: Pin<Box<dyn AsyncWrite + Send>>) -> io::Result<()> {
    let mut stdin = tokio::io::stdin();
    let mut buffer = [0_u8; 8192];

    loop {
        let bytes_read = stdin.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        let bytes = buffer.get(..bytes_read).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "read size exceeded stdin buffer length",
            )
        })?;
        write_all_pinned_async(input.as_mut(), bytes).await?;
    }

    flush_pinned_async(input.as_mut()).await
}

async fn write_all_pinned_async(
    mut writer: Pin<&mut (dyn AsyncWrite + Send)>,
    mut bytes: &[u8],
) -> io::Result<()> {
    while !bytes.is_empty() {
        let written = std::future::poll_fn(|cx| writer.as_mut().poll_write(cx, bytes)).await?;
        if written == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "failed to forward stdin to exec session",
            ));
        }
        bytes = bytes.get(written..).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "write size exceeded buffered stdin chunk length",
            )
        })?;
    }

    Ok(())
}

async fn flush_pinned_async(mut writer: Pin<&mut (dyn AsyncWrite + Send)>) -> io::Result<()> {
    std::future::poll_fn(|cx| writer.as_mut().poll_flush(cx)).await
}
