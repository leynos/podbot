//! Terminal-size and resize-signal helpers for interactive exec sessions.

use std::io::IsTerminal;
use std::process::{Command, Output};

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
            log_non_terminal_stdio();
            return None;
        }

        let output = stty_size_output()?;
        let stdout = decode_stty_stdout(output.stdout)?;
        parse_stty_size_logged(&stdout)
    }
}

/// Log why terminal size detection is skipped when local stdio is not a TTY.
fn log_non_terminal_stdio() {
    tracing::debug!(
        stdin_is_terminal = std::io::stdin().is_terminal(),
        stdout_is_terminal = std::io::stdout().is_terminal(),
        "terminal size detection skipped because stdin or stdout is not a terminal"
    );
}

/// Run `stty size`, returning its output only when the command succeeds.
///
/// Emits a debug event and returns `None` when `stty` cannot be executed or
/// exits with a non-zero status.
fn stty_size_output() -> Option<Output> {
    let output = Command::new(STTY_COMMAND)
        .arg("size")
        .output()
        .map_err(|error| {
            tracing::debug!(
                command = STTY_COMMAND,
                args = "size",
                error = %error,
                "failed to execute stty to detect terminal size"
            );
            error
        })
        .ok()?;

    if output.status.success() {
        Some(output)
    } else {
        log_stty_status_failure(&output);
        None
    }
}

/// Emit a debug event describing a non-zero `stty size` exit status.
fn log_stty_status_failure(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    tracing::debug!(
        status = %output.status,
        exit_code = ?output.status.code(),
        stderr = stderr.trim(),
        stdout = stdout.trim(),
        "stty size exited with a non-zero status"
    );
}

/// Decode `stty size` stdout as UTF-8, logging and returning `None` on failure.
fn decode_stty_stdout(stdout: Vec<u8>) -> Option<String> {
    match String::from_utf8(stdout) {
        Ok(decoded) => Some(decoded),
        Err(error) => {
            tracing::debug!(
                error = %error,
                stdout_bytes = ?error.as_bytes(),
                "stty size output is not valid UTF-8"
            );
            None
        }
    }
}

/// Parse `stty size` output, logging and returning `None` when it is malformed.
fn parse_stty_size_logged(stdout: &str) -> Option<TerminalSize> {
    let size = parse_stty_size(stdout);
    if size.is_none() {
        tracing::debug!(stdout = ?stdout, "failed to parse stty size output");
    }
    size
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
    //! Unit tests for terminal size discovery and resize handling.

    use std::sync::{Arc, Mutex};

    use super::{decode_stty_stdout, parse_stty_size, parse_stty_size_logged};
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

    /// Capture `DEBUG`-level `tracing` output emitted by `run_test`.
    fn capture_debug_logs(run_test: impl FnOnce()) -> Result<String, String> {
        let buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
        let writer = SharedLogBuffer {
            buffer: Arc::clone(&buffer),
        };
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .without_time()
            .with_ansi(false)
            .with_writer(writer)
            .finish();

        tracing::subscriber::with_default(subscriber, run_test);

        let bytes = buffer
            .lock()
            .map_err(|error| format!("log buffer lock should not be poisoned: {error}"))?
            .clone();
        String::from_utf8(bytes).map_err(|error| format!("logs should be UTF-8: {error}"))
    }

    #[derive(Clone)]
    struct SharedLogBuffer {
        buffer: Arc<Mutex<Vec<u8>>>,
    }

    impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for SharedLogBuffer {
        type Writer = SharedLogWriter;

        fn make_writer(&'writer self) -> Self::Writer {
            SharedLogWriter {
                buffer: Arc::clone(&self.buffer),
            }
        }
    }

    struct SharedLogWriter {
        buffer: Arc<Mutex<Vec<u8>>>,
    }

    impl std::io::Write for SharedLogWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.buffer
                .lock()
                .map_err(|error| std::io::Error::other(format!("log buffer poisoned: {error}")))?
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn decode_stty_stdout_logs_invalid_utf8_and_returns_none() {
        let logs = capture_debug_logs(|| {
            assert!(
                decode_stty_stdout(vec![0xff, 0xfe]).is_none(),
                "invalid UTF-8 should not decode"
            );
        })
        .expect("debug logs should be captured");

        assert!(
            logs.contains("stty size output is not valid UTF-8"),
            "decode failure should be logged: {logs}"
        );
        assert!(
            logs.contains("stdout_bytes"),
            "decode failure should include the raw bytes: {logs}"
        );
        assert!(
            logs.contains("invalid utf-8"),
            "decode failure should include the UTF-8 error: {logs}"
        );
    }

    #[test]
    fn parse_stty_size_logged_logs_unparsable_output_and_returns_none() {
        let logs = capture_debug_logs(|| {
            assert!(
                parse_stty_size_logged("not a size").is_none(),
                "unparsable output should not produce a size"
            );
        })
        .expect("debug logs should be captured");

        assert!(
            logs.contains("failed to parse stty size output"),
            "parse failure should be logged: {logs}"
        );
        assert!(
            logs.contains("not a size"),
            "parse failure should include the raw stdout: {logs}"
        );
    }
}
