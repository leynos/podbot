//! Git identity reading and container configuration.
//!
//! This module reads `user.name` and `user.email` from the host Git
//! configuration and applies them inside a container via
//! `git config --global`. Missing identity fields produce warnings rather
//! than errors, following the roadmap requirement that absent identity must
//! not block agent execution.
//!
//! Host Git reading is abstracted behind the [`GitIdentityReader`] trait so
//! tests can inject deterministic values without depending on the host Git
//! installation. Container application reuses the existing
//! [`super::exec::ContainerExecClient`] trait seam.

use std::process::Command;

use super::exec::ContainerExecClient;
use super::EngineConnector;
use crate::error::{ContainerError, PodbotError};

// =============================================================================
// GitIdentity data type
// =============================================================================

/// Git identity fields read from the host configuration.
///
/// Both fields are optional because the host may have neither, one, or both
/// configured. Consumers should check individual fields rather than treating
/// the struct as all-or-nothing.
///
/// # Examples
///
/// ```
/// use podbot::engine::GitIdentity;
///
/// let identity = GitIdentity::new(Some("Alice".into()), Some("alice@example.com".into()));
/// assert!(identity.is_complete());
/// assert!(!identity.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitIdentity {
    name: Option<String>,
    email: Option<String>,
}

impl GitIdentity {
    /// Create a new Git identity from optional name and email values.
    #[must_use]
    pub fn new(name: Option<String>, email: Option<String>) -> Self {
        Self { name, email }
    }

    /// Return the configured user name, if any.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Return the configured user email, if any.
    #[must_use]
    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    /// Return true when a user name is configured.
    #[must_use]
    pub fn has_name(&self) -> bool {
        self.name.is_some()
    }

    /// Return true when a user email is configured.
    #[must_use]
    pub fn has_email(&self) -> bool {
        self.email.is_some()
    }

    /// Return true when neither name nor email is configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.email.is_none()
    }

    /// Return true when both name and email are configured.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.name.is_some() && self.email.is_some()
    }
}

impl Default for GitIdentity {
    fn default() -> Self {
        Self {
            name: None,
            email: None,
        }
    }
}

// =============================================================================
// GitIdentityReader trait
// =============================================================================

/// Reads Git identity from the host configuration.
///
/// Implementations query the host environment for `user.name` and
/// `user.email`. The trait enables dependency injection so tests can
/// provide deterministic values without depending on the host Git
/// installation.
pub trait GitIdentityReader {
    /// Read the Git identity from the host configuration.
    fn read_git_identity(&self) -> GitIdentity;
}

// =============================================================================
// SystemGitIdentityReader
// =============================================================================

/// Production implementation that reads Git identity using `git config --get`.
///
/// This reader invokes `git config --get user.name` and
/// `git config --get user.email` as subprocesses. Non-zero exit codes
/// (indicating the key is not set) and command failures (Git not installed)
/// both result in `None` for the affected field.
pub struct SystemGitIdentityReader;

impl SystemGitIdentityReader {
    /// Create a new system Git identity reader.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for SystemGitIdentityReader {
    fn default() -> Self {
        Self::new()
    }
}

impl GitIdentityReader for SystemGitIdentityReader {
    fn read_git_identity(&self) -> GitIdentity {
        let name = read_git_config_value("user.name");
        let email = read_git_config_value("user.email");
        GitIdentity::new(name, email)
    }
}

/// Run `git config --get <key>` and return the trimmed output on success.
///
/// Returns `None` when the command exits with a non-zero code (key not set)
/// or cannot be executed (Git not installed).
fn read_git_config_value(key: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--get", key])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() {
        return None;
    }

    Some(value)
}

// =============================================================================
// GitIdentityResult
// =============================================================================

/// Outcome of applying Git identity to a container.
///
/// Reports which fields were successfully applied and which were skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitIdentityResult {
    name_applied: bool,
    email_applied: bool,
    warnings: Vec<String>,
}

impl GitIdentityResult {
    /// Return true if the user name was applied to the container.
    #[must_use]
    pub const fn name_applied(&self) -> bool {
        self.name_applied
    }

    /// Return true if the user email was applied to the container.
    #[must_use]
    pub const fn email_applied(&self) -> bool {
        self.email_applied
    }

    /// Return warnings emitted during identity configuration.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Return true if no fields were applied.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !self.name_applied && !self.email_applied
    }
}

// =============================================================================
// Container Git identity application
// =============================================================================

impl EngineConnector {
    /// Read host Git identity and apply it within a container.
    ///
    /// Reads `user.name` and `user.email` from the host using the provided
    /// reader, then executes `git config --global` for each present field
    /// inside the container. Missing fields and exec failures produce
    /// warnings rather than errors.
    ///
    /// # Errors
    ///
    /// This function does not return errors for missing identity or exec
    /// failures. It returns `ContainerError::ExecFailed` only when the
    /// container ID is invalid (empty).
    pub async fn configure_git_identity_async<C, R>(
        client: &C,
        container_id: &str,
        reader: &R,
    ) -> Result<GitIdentityResult, PodbotError>
    where
        C: ContainerExecClient,
        R: GitIdentityReader,
    {
        if container_id.trim().is_empty() {
            return Err(PodbotError::from(ContainerError::ExecFailed {
                container_id: String::from(container_id),
                message: String::from("container ID must not be empty"),
            }));
        }

        let identity = reader.read_git_identity();
        apply_git_identity_async(client, container_id, &identity).await
    }

    /// Apply a pre-read Git identity within a container.
    ///
    /// This variant accepts an already-read [`GitIdentity`] for callers
    /// that have obtained the identity separately.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::ExecFailed` only when the container ID is
    /// invalid (empty).
    pub async fn apply_git_identity_async<C: ContainerExecClient>(
        client: &C,
        container_id: &str,
        identity: &GitIdentity,
    ) -> Result<GitIdentityResult, PodbotError> {
        if container_id.trim().is_empty() {
            return Err(PodbotError::from(ContainerError::ExecFailed {
                container_id: String::from(container_id),
                message: String::from("container ID must not be empty"),
            }));
        }

        apply_git_identity_async(client, container_id, identity).await
    }
}

/// Apply Git identity fields to a container, collecting warnings.
async fn apply_git_identity_async<C: ContainerExecClient>(
    client: &C,
    container_id: &str,
    identity: &GitIdentity,
) -> Result<GitIdentityResult, PodbotError> {
    let mut warnings = Vec::new();
    let mut name_applied = false;
    let mut email_applied = false;

    if identity.is_empty() {
        warnings.push(String::from(
            "no Git identity configured on the host; \
             commits in the container will use the container default",
        ));
        return Ok(GitIdentityResult {
            name_applied,
            email_applied,
            warnings,
        });
    }

    if let Some(name) = identity.name() {
        name_applied =
            apply_single_config(client, container_id, "user.name", name, &mut warnings).await;
    } else {
        warnings.push(String::from(
            "host Git user.name is not configured; skipping",
        ));
    }

    if let Some(email) = identity.email() {
        email_applied =
            apply_single_config(client, container_id, "user.email", email, &mut warnings).await;
    } else {
        warnings.push(String::from(
            "host Git user.email is not configured; skipping",
        ));
    }

    Ok(GitIdentityResult {
        name_applied,
        email_applied,
        warnings,
    })
}

/// Execute a single `git config --global <key> <value>` in the container.
///
/// Returns `true` on success. On failure, appends a warning and returns
/// `false`.
async fn apply_single_config<C: ContainerExecClient>(
    client: &C,
    container_id: &str,
    key: &str,
    value: &str,
    warnings: &mut Vec<String>,
) -> bool {
    let options = bollard::exec::CreateExecOptions::<String> {
        attach_stdout: Some(false),
        attach_stderr: Some(false),
        attach_stdin: Some(false),
        tty: Some(false),
        cmd: Some(vec![
            String::from("git"),
            String::from("config"),
            String::from("--global"),
            String::from(key),
            String::from(value),
        ]),
        ..bollard::exec::CreateExecOptions::default()
    };

    let create_result = client.create_exec(container_id, options).await;
    let exec_id = match create_result {
        Ok(result) => result.id,
        Err(error) => {
            warnings.push(format!(
                "failed to create exec for git config {key}: {error}"
            ));
            return false;
        }
    };

    let start_result = client
        .start_exec(
            &exec_id,
            Some(bollard::exec::StartExecOptions {
                detach: true,
                tty: false,
                output_capacity: None,
            }),
        )
        .await;

    if let Err(error) = start_result {
        warnings.push(format!(
            "failed to start exec for git config {key}: {error}"
        ));
        return false;
    }

    // Poll for completion.
    if let Err(error) = wait_for_exec_completion(client, &exec_id).await {
        warnings.push(format!(
            "failed to inspect exec for git config {key}: {error}"
        ));
        return false;
    }

    true
}

/// Wait for a detached exec to complete by polling inspect.
async fn wait_for_exec_completion<C: ContainerExecClient>(
    client: &C,
    exec_id: &str,
) -> Result<(), String> {
    loop {
        let inspect = client
            .inspect_exec(exec_id)
            .await
            .map_err(|e| format!("{e}"))?;

        if let Some(running) = inspect.running {
            if !running {
                // Check exit code for non-zero (Git config failure).
                if let Some(code) = inspect.exit_code {
                    if code != 0 {
                        return Err(format!(
                            "git config exited with code {code}"
                        ));
                    }
                }
                return Ok(());
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(
            super::exec::EXEC_INSPECT_POLL_INTERVAL_MS,
        ))
        .await;
    }
}

#[cfg(test)]
mod tests;
