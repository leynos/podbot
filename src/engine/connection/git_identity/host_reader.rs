//! Host-side Git identity reading.
//!
//! Reads `user.name` and `user.email` from the host Git configuration
//! by running `git config --get` commands. The command runner is
//! injected for testability.

use std::io;
use std::process::Output;

/// Runs a command on the host and returns its output.
///
/// This trait abstracts host command execution so tests can inject
/// mock responses without requiring a real `git` binary.
pub trait HostCommandRunner {
    /// Execute `program` with `args` and return the captured output.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the command cannot be spawned.
    fn run_command(&self, program: &str, args: &[&str]) -> io::Result<Output>;
}

/// Production implementation using `std::process::Command`.
pub struct SystemCommandRunner;

impl HostCommandRunner for SystemCommandRunner {
    fn run_command(&self, program: &str, args: &[&str]) -> io::Result<Output> {
        std::process::Command::new(program).args(args).output()
    }
}

/// Identity fields read from the host Git configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostGitIdentity {
    /// `user.name` value, if configured on the host.
    pub name: Option<String>,
    /// `user.email` value, if configured on the host.
    pub email: Option<String>,
}

/// Read Git `user.name` and `user.email` from the host.
///
/// Returns `None` values for fields that are not configured rather
/// than failing. The caller decides how to handle missing fields.
pub fn read_host_git_identity(runner: &impl HostCommandRunner) -> HostGitIdentity {
    HostGitIdentity {
        name: read_git_config_value(runner, "user.name"),
        email: read_git_config_value(runner, "user.email"),
    }
}

fn read_git_config_value(runner: &impl HostCommandRunner, key: &str) -> Option<String> {
    let output = runner.run_command("git", &["config", "--get", key]).ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();

    if value.is_empty() { None } else { Some(value) }
}
