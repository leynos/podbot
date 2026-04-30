//! Library-owned request type for interactive agent runs.
//!
//! This module keeps the semantic `podbot run` request independent from the
//! Clap-backed CLI adapter so Rust embedders can construct the same operation
//! directly through `podbot::api`.

use crate::error::{ConfigError, Result as PodbotResult};

/// Request to run an AI agent against a repository branch.
///
/// # Examples
///
/// ```rust
/// use podbot::api::RunRequest;
///
/// let request = RunRequest::new("owner/name", "main")?;
/// assert_eq!(request.repository(), "owner/name");
/// assert_eq!(request.branch(), "main");
/// # Ok::<(), podbot::error::PodbotError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRequest {
    repository: String,
    branch: String,
}

impl RunRequest {
    /// Creates a request for an interactive agent run.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::InvalidValue` when either value is empty or
    /// whitespace only.
    pub fn new(
        repository_value: impl Into<String>,
        branch_value: impl Into<String>,
    ) -> PodbotResult<Self> {
        let repository = repository_value.into();
        let branch = branch_value.into();

        validate_non_empty("run.repository", &repository)?;
        validate_non_empty("run.branch", &branch)?;

        Ok(Self { repository, branch })
    }

    /// Repository to clone in `owner/name` format.
    #[must_use]
    pub fn repository(&self) -> &str {
        &self.repository
    }

    /// Branch to check out before launching the agent.
    #[must_use]
    pub fn branch(&self) -> &str {
        &self.branch
    }
}

fn validate_non_empty(field: &str, value: &str) -> PodbotResult<()> {
    if value.trim().is_empty() {
        return Err(ConfigError::InvalidValue {
            field: field.to_owned(),
            reason: format!("{field} must not be empty"),
        }
        .into());
    }

    Ok(())
}
