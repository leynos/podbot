//! Terminal-size and resize-signal helpers for interactive exec sessions.

use std::io::IsTerminal;
use std::process::Command;

use bollard::exec::ResizeExecOptions;

use super::{ContainerExecClient, ExecRequest, exec_failed};
use crate::error::PodbotError;

/// Host utility used to query terminal rows and columns when stdio is a
/// terminal.
const STTY_COMMAND: &str = "stty";

/// Terminal dimensions passed to the daemon without swapping rows and columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TerminalSize {
    /// Number of character columns expected by the daemon resize operation.
    pub(super) width: u16,
    /// Number of character rows expected by the daemon resize operation.
    pub(super) height: u16,
}

/// Injectable source of terminal dimensions for attached-session resizing.
pub(super) trait TerminalSizeProvider {
    /// Return dimensions when available, or skip resizing when no size can be
    /// read.
    /// For example, a non-terminal input produces `None` in the system
    /// provider.
    fn terminal_size(&self) -> Option<TerminalSize>;
}

/// Host terminal-size provider using `stty size` when stdin and stdout are
/// terminals.
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

/// Resize exec to the supplied terminal dimensions when a size is available.
/// For example, `None` from the provider succeeds without contacting the
/// daemon.
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

/// Parse `stty size` rows and columns into daemon width and height.
/// For example, `"42 120\n"` yields width 120 and height 42.
fn parse_stty_size(output: &str) -> Option<TerminalSize> {
    let mut parts = output.split_whitespace();
    let height = parts.next()?.parse::<u16>().ok()?;
    let width = parts.next()?.parse::<u16>().ok()?;
    Some(TerminalSize { width, height })
}

/// Require both local input and output to be terminals before querying `stty`.
/// For example, redirected stdout yields `false` even with terminal input.
fn local_stdio_is_terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Subscribe to Unix window changes only for requests that allocate a TTY.
/// For example, a non-TTY request yields `Ok(None)` without a signal listener.
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

/// Await the next resize signal when a listener exists.
/// For example, `None` completes immediately; callers disable that select
/// branch.
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
    //! Unit tests for terminal size discovery and resize handling.

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
