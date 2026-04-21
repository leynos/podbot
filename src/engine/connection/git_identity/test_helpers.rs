//! Shared test helpers for Git identity unit tests.
//!
//! Provides platform-independent `ExitStatus` construction and
//! `std::process::Output` factories used across `tests.rs` and
//! `container_configurator.rs` test modules.

use std::process::{ExitStatus, Output};

/// Platform-independent exit status construction.
#[cfg(unix)]
pub fn exit_status(code: i32) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(code << 8)
}

/// Platform-independent exit status construction.
#[cfg(windows)]
pub fn exit_status(code: i32) -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(code as u32)
}

/// Build a successful `Output` with the given stdout content.
pub fn success_output(stdout: &str) -> Output {
    Output {
        status: exit_status(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
    }
}

/// Build a failed `Output` (exit code 1) with a generic stderr message.
pub fn failure_output() -> Output {
    Output {
        status: exit_status(1),
        stdout: Vec::new(),
        stderr: b"error".to_vec(),
    }
}
