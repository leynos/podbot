//! Attached exec-session stream and IO forwarding helpers.

use std::io;
use std::pin::Pin;
use std::time::Duration;

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::{Stream, StreamExt};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::task::JoinHandle;
use tokio::time::sleep;

use super::terminal::{TerminalSizeProvider, resize_exec_to_current_terminal_async};
#[cfg(unix)]
use super::terminal::{maybe_sigwinch_listener, wait_for_sigwinch};
use super::{ContainerExecClient, EXEC_INSPECT_POLL_INTERVAL_MS, ExecRequest, exec_failed};
use crate::error::PodbotError;

#[expect(
    clippy::too_many_arguments,
    reason = "entrypoint signature mirrors Bollard attached exec state requirements"
)]
pub(super) async fn run_attached_session_async<C: ContainerExecClient, P: TerminalSizeProvider>(
    client: &C,
    request: &ExecRequest,
    exec_id: &str,
    mut output: Pin<Box<dyn Stream<Item = Result<LogOutput, BollardError>> + Send>>,
    input: Pin<Box<dyn AsyncWrite + Send>>,
    size_provider: &P,
) -> Result<(), PodbotError> {
    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();
    run_attached_session_with_stdio_async(
        client,
        request,
        exec_id,
        &mut output,
        input,
        size_provider,
        &mut stdout,
        &mut stderr,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "attached-session orchestration needs stream, IO, and resize state together"
)]
async fn run_attached_session_with_stdio_async<C: ContainerExecClient, P: TerminalSizeProvider>(
    client: &C,
    request: &ExecRequest,
    exec_id: &str,
    output: &mut Pin<Box<dyn Stream<Item = Result<LogOutput, BollardError>> + Send>>,
    input: Pin<Box<dyn AsyncWrite + Send>>,
    size_provider: &P,
    stdout: &mut tokio::io::Stdout,
    stderr: &mut tokio::io::Stderr,
) -> Result<(), PodbotError> {
    let stdin_task = spawn_stdin_forwarding_task(input);
    let session_result = run_output_session_with_resize_init_async(
        client,
        request,
        exec_id,
        output,
        size_provider,
        stdout,
        stderr,
    )
    .await;
    stop_stdin_forwarding_task(stdin_task).await;
    session_result
}

fn spawn_stdin_forwarding_task(
    input: Pin<Box<dyn AsyncWrite + Send>>,
) -> JoinHandle<io::Result<()>> {
    tokio::spawn(async move { forward_stdin_to_exec_async(input).await })
}

async fn stop_stdin_forwarding_task(stdin_task: JoinHandle<io::Result<()>>) {
    stdin_task.abort();
    drop(stdin_task.await);
}

#[expect(
    clippy::too_many_arguments,
    reason = "platform-specific session loop needs stream, IO, and resize dependencies"
)]
async fn run_output_session_with_resize_init_async<
    C: ContainerExecClient,
    P: TerminalSizeProvider,
>(
    client: &C,
    request: &ExecRequest,
    exec_id: &str,
    output: &mut Pin<Box<dyn Stream<Item = Result<LogOutput, BollardError>> + Send>>,
    size_provider: &P,
    stdout: &mut tokio::io::Stdout,
    stderr: &mut tokio::io::Stderr,
) -> Result<(), PodbotError> {
    #[cfg(unix)]
    {
        let mut sigwinch =
            initialize_resize_handling_async(client, request, exec_id, size_provider).await?;
        return run_output_loop_with_resize_async(
            client,
            request,
            exec_id,
            size_provider,
            output,
            stdout,
            stderr,
            &mut sigwinch,
        )
        .await;
    }

    #[cfg(not(unix))]
    {
        initialize_resize_handling_async(client, request, exec_id, size_provider).await?;
        run_output_loop_async(request, output, stdout, stderr).await
    }
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
    tokio::io::copy(&mut stdin, &mut input).await?;
    input.flush().await
}

#[cfg(unix)]
async fn initialize_resize_handling_async<C: ContainerExecClient, P: TerminalSizeProvider>(
    client: &C,
    request: &ExecRequest,
    exec_id: &str,
    size_provider: &P,
) -> Result<Option<tokio::signal::unix::Signal>, PodbotError> {
    let sigwinch = maybe_sigwinch_listener(request)?;
    maybe_resize_exec_async(client, request, exec_id, size_provider).await?;
    Ok(sigwinch)
}

#[cfg(not(unix))]
async fn initialize_resize_handling_async<C: ContainerExecClient, P: TerminalSizeProvider>(
    client: &C,
    request: &ExecRequest,
    exec_id: &str,
    size_provider: &P,
) -> Result<(), PodbotError> {
    maybe_resize_exec_async(client, request, exec_id, size_provider).await
}

async fn maybe_resize_exec_async<C: ContainerExecClient, P: TerminalSizeProvider>(
    client: &C,
    request: &ExecRequest,
    exec_id: &str,
    size_provider: &P,
) -> Result<(), PodbotError> {
    if request.tty() {
        resize_exec_to_current_terminal_async(
            client,
            request.container_id(),
            exec_id,
            size_provider,
        )
        .await?;
    }

    Ok(())
}

#[cfg(unix)]
#[expect(
    clippy::too_many_arguments,
    reason = "stream and signal loop requires explicit IO and resize state"
)]
#[expect(
    clippy::integer_division_remainder_used,
    reason = "false positive triggered inside tokio::select! expansion"
)]
async fn run_output_loop_with_resize_async<C: ContainerExecClient, P: TerminalSizeProvider>(
    client: &C,
    request: &ExecRequest,
    exec_id: &str,
    size_provider: &P,
    output: &mut Pin<Box<dyn Stream<Item = Result<LogOutput, BollardError>> + Send>>,
    stdout: &mut tokio::io::Stdout,
    stderr: &mut tokio::io::Stderr,
    sigwinch: &mut Option<tokio::signal::unix::Signal>,
) -> Result<(), PodbotError> {
    loop {
        tokio::select! {
            maybe_chunk = output.next() => {
                if !write_exec_output_chunk(request.container_id(), maybe_chunk, stdout, stderr).await? {
                    return Ok(());
                }
            }
            () = wait_for_sigwinch(sigwinch), if sigwinch.is_some() => {
                resize_exec_to_current_terminal_async(client, request.container_id(), exec_id, size_provider).await?;
            }
        }
    }
}

#[cfg(not(unix))]
async fn run_output_loop_async(
    request: &ExecRequest,
    output: &mut Pin<Box<dyn Stream<Item = Result<LogOutput, BollardError>> + Send>>,
    stdout: &mut tokio::io::Stdout,
    stderr: &mut tokio::io::Stderr,
) -> Result<(), PodbotError> {
    loop {
        let maybe_chunk = output.next().await;
        if !write_exec_output_chunk(request.container_id(), maybe_chunk, stdout, stderr).await? {
            return Ok(());
        }
    }
}
