//! Terminal-size and resize-signal helpers for interactive exec sessions.

use std::io::IsTerminal;
use std::process::Command;

use bollard::exec::ResizeExecOptions;

use super::{ContainerExecClient, ExecRequest, exec_failed};
use crate::error::PodbotError;

const STTY_COMMAND: &str = "stty";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TerminalSize {
    pub(super) width: u16,
    pub(super) height: u16,
}

pub(super) trait TerminalSizeProvider {
    fn terminal_size(&self) -> Option<TerminalSize>;
}

pub(super) struct SystemTerminalSizeProvider;

impl TerminalSizeProvider for SystemTerminalSizeProvider {
    fn terminal_size(&self) -> Option<TerminalSize> {
        if !local_stdio_is_terminal() {
            return None;
        }

        let output = Command::new(STTY_COMMAND).arg("size").output().ok()?;
        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8(output.stdout).ok()?;
        parse_stty_size(&stdout)
    }
}

pub(super) async fn resize_exec_to_current_terminal_async<
    C: ContainerExecClient,
    P: TerminalSizeProvider,
>(
    client: &C,
    container_id: &str,
    exec_id: &str,
    size_provider: &P,
) -> Result<(), PodbotError> {
    let Some(size) = size_provider.terminal_size() else {
        return Ok(());
    };

    client
        .resize_exec(
            exec_id,
            ResizeExecOptions {
                width: size.width,
                height: size.height,
            },
        )
        .await
        .map_err(|error| exec_failed(container_id, format!("resize exec failed: {error}")))?;

    Ok(())
}

fn parse_stty_size(output: &str) -> Option<TerminalSize> {
    let mut parts = output.split_whitespace();
    let height = parts.next()?.parse::<u16>().ok()?;
    let width = parts.next()?.parse::<u16>().ok()?;
    Some(TerminalSize { width, height })
}

fn local_stdio_is_terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

#[cfg(unix)]
pub(super) fn maybe_sigwinch_listener(
    request: &ExecRequest,
) -> Result<Option<tokio::signal::unix::Signal>, PodbotError> {
    if !request.tty() {
        return Ok(None);
    }

    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())
        .map(Some)
        .map_err(|error| {
            exec_failed(
                request.container_id(),
                format!("failed to subscribe to SIGWINCH: {error}"),
            )
        })
}

#[cfg(unix)]
pub(super) async fn wait_for_sigwinch(signal: &mut Option<tokio::signal::unix::Signal>) {
    if let Some(listener) = signal.as_mut() {
        // `wait_for_sigwinch` only needs to await the `listener` notification on `signal`.
        // The returned value is intentionally ignored.
        let _ = listener.recv().await;
    }
}

#[cfg(test)]
mod tests {
    use super::parse_stty_size;
    use rstest::rstest;

    #[rstest]
    #[case("42 120\n", Some((120, 42)))]
    #[case("42 120   \n", Some((120, 42)))]
    #[case("  42   120", Some((120, 42)))]
    #[case("42\n120", Some((120, 42)))]
    #[case("120 42", Some((42, 120)))]
    #[case("foo", None)]
    #[case("42", None)]
    #[case("", None)]
    #[case("0 0", Some((0, 0)))]
    #[case("65535 65535", Some((65535, 65535)))]
    fn parse_stty_size_returns_expected_dimensions(
        #[case] input: &str,
        #[case] expected: Option<(u16, u16)>,
    ) {
        let parsed = parse_stty_size(input);
        assert_eq!(parsed.map(|size| (size.width, size.height)), expected);
    }
}
