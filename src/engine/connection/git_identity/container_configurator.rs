//! Container-side Git identity configuration.
//!
//! Executes `git config --global user.name` and
//! `git config --global user.email` within a running container using
//! the injected [`ContainerExecClient`].

use crate::engine::{ContainerExecClient, EngineConnector, ExecMode, ExecRequest};
use crate::error::PodbotError;

use super::host_reader::HostGitIdentity;

/// Warning message for missing Git user.name configuration.
const MISSING_NAME_WARNING: &str = "git user.name is not configured on the host";

/// Warning message for missing Git user.email configuration.
const MISSING_EMAIL_WARNING: &str = "git user.email is not configured on the host";

/// Outcome of configuring Git identity in a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitIdentityResult {
    /// Both name and email were configured successfully.
    Configured {
        /// The configured `user.name`.
        name: String,
        /// The configured `user.email`.
        email: String,
    },
    /// Only some identity fields were configured.
    Partial {
        /// The configured `user.name`, if set.
        name: Option<String>,
        /// The configured `user.email`, if set.
        email: Option<String>,
        /// Warning messages for missing fields.
        warnings: Vec<String>,
    },
    /// No identity fields were available on the host.
    NoneConfigured {
        /// Warning messages explaining the absence.
        warnings: Vec<String>,
    },
}

/// Classifies the completeness of host Git identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentityCompleteness {
    /// Both name and email are present.
    Complete,
    /// At least one of name or email is missing.
    Partial,
    /// Neither name nor email is present.
    None,
}

/// Determines the completeness of the host Git identity.
const fn classify_identity(identity: &HostGitIdentity) -> IdentityCompleteness {
    match (&identity.name, &identity.email) {
        (None, None) => IdentityCompleteness::None,
        (Some(_), Some(_)) => IdentityCompleteness::Complete,
        _ => IdentityCompleteness::Partial,
    }
}

/// Configure Git identity in a container using host-read values.
///
/// Executes `git config --global user.name` and/or
/// `git config --global user.email` for each value present in
/// `identity`. Missing values produce warnings rather than errors.
///
/// # Errors
///
/// Returns `ContainerError::ExecFailed` if a `git config` command
/// fails inside the container.
pub fn configure_git_identity<C: ContainerExecClient>(
    runtime: &tokio::runtime::Handle,
    client: &C,
    container_id: &str,
    identity: &HostGitIdentity,
) -> Result<GitIdentityResult, PodbotError> {
    match classify_identity(identity) {
        IdentityCompleteness::None => Ok(GitIdentityResult::NoneConfigured {
            warnings: vec![
                String::from(MISSING_NAME_WARNING),
                String::from(MISSING_EMAIL_WARNING),
            ],
        }),
        IdentityCompleteness::Complete => {
            configure_complete_identity(runtime, client, container_id, identity)
        }
        IdentityCompleteness::Partial => {
            configure_partial_identity(runtime, client, container_id, identity)
        }
    }
}

fn configure_complete_identity<C: ContainerExecClient>(
    runtime: &tokio::runtime::Handle,
    client: &C,
    container_id: &str,
    identity: &HostGitIdentity,
) -> Result<GitIdentityResult, PodbotError> {
    // Pattern match to extract values; this function is only called
    // when classify_identity returns Complete
    let (Some(name), Some(email)) = (&identity.name, &identity.email) else {
        // This should never happen if classify_identity is correct, but we
        // handle it gracefully by falling back to partial configuration
        return configure_partial_identity(runtime, client, container_id, identity);
    };

    let params = GitConfigParams {
        runtime,
        client,
        container_id,
    };
    set_git_config(&params, "user.name", name)?;
    set_git_config(&params, "user.email", email)?;
    Ok(GitIdentityResult::Configured {
        name: name.clone(),
        email: email.clone(),
    })
}

fn configure_partial_identity<C: ContainerExecClient>(
    runtime: &tokio::runtime::Handle,
    client: &C,
    container_id: &str,
    identity: &HostGitIdentity,
) -> Result<GitIdentityResult, PodbotError> {
    let mut warnings = Vec::new();
    let params = GitConfigParams {
        runtime,
        client,
        container_id,
    };

    if let Some(name) = &identity.name {
        set_git_config(&params, "user.name", name)?;
    } else {
        warnings.push(String::from(MISSING_NAME_WARNING));
    }

    if let Some(email) = &identity.email {
        set_git_config(&params, "user.email", email)?;
    } else {
        warnings.push(String::from(MISSING_EMAIL_WARNING));
    }

    Ok(GitIdentityResult::Partial {
        name: identity.name.clone(),
        email: identity.email.clone(),
        warnings,
    })
}

struct GitConfigParams<'a, C: ContainerExecClient> {
    runtime: &'a tokio::runtime::Handle,
    client: &'a C,
    container_id: &'a str,
}

fn set_git_config<C: ContainerExecClient>(
    params: &GitConfigParams<'_, C>,
    key: &str,
    value: &str,
) -> Result<(), PodbotError> {
    let command = vec![
        String::from("git"),
        String::from("config"),
        String::from("--global"),
        String::from(key),
        String::from(value),
    ];
    let request = ExecRequest::new(params.container_id, command, ExecMode::Detached)?;
    let result = EngineConnector::exec(params.runtime, params.client, &request)?;

    if result.exit_code() != 0 {
        return Err(super::git_identity_exec_failed(
            params.container_id,
            format!(
                "git config --global {key} failed with exit code {}",
                result.exit_code()
            ),
        ));
    }

    Ok(())
}
