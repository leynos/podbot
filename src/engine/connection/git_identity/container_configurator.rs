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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::connection::git_identity::host_reader::HostGitIdentity;
    use crate::engine::{CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture};

    use bollard::exec::{CreateExecOptions, ResizeExecOptions, StartExecOptions};
    use mockall::mock;

    mock! {
        ExecClient {}
        impl ContainerExecClient for ExecClient {
            fn create_exec(
                &self,
                container_id: &str,
                options: CreateExecOptions<String>,
            ) -> CreateExecFuture<'_>;
            fn start_exec(
                &self,
                exec_id: &str,
                options: Option<StartExecOptions>,
            ) -> StartExecFuture<'_>;
            fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
            fn resize_exec(
                &self,
                exec_id: &str,
                options: ResizeExecOptions,
            ) -> ResizeExecFuture<'_>;
        }
    }

    fn make_runtime() -> (tokio::runtime::Runtime, tokio::runtime::Handle) {
        let rt = tokio::runtime::Runtime::new().expect("test requires a Tokio runtime");
        let handle = rt.handle().clone();
        (rt, handle)
    }

    fn make_exec_client(exit_code: i64) -> MockExecClient {
        let mut client = MockExecClient::new();
        client.expect_create_exec().returning(|_, _| {
            Box::pin(async {
                Ok(bollard::exec::CreateExecResults {
                    id: String::from("exec-1"),
                })
            })
        });
        client
            .expect_start_exec()
            .returning(|_, _| Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) }));
        client.expect_inspect_exec().returning(move |_| {
            Box::pin(async move {
                Ok(bollard::models::ExecInspectResponse {
                    exit_code: Some(exit_code),
                    running: Some(false),
                    ..Default::default()
                })
            })
        });
        client
    }

    #[test]
    fn returns_none_configured_when_both_fields_absent() {
        let (_rt, handle) = make_runtime();
        // No exec expectations — no container commands should be issued.
        let client = MockExecClient::new();
        let identity = HostGitIdentity {
            name: None,
            email: None,
        };

        let result = configure_git_identity(&handle, &client, "c1", &identity)
            .expect("should succeed with NoneConfigured");

        assert!(
            matches!(result, GitIdentityResult::NoneConfigured { .. }),
            "Expected NoneConfigured, got {result:?}"
        );
        if let GitIdentityResult::NoneConfigured { warnings } = result {
            assert!(warnings.iter().any(|w| w.contains("user.name")));
            assert!(warnings.iter().any(|w| w.contains("user.email")));
        }
    }

    #[test]
    fn returns_configured_when_both_fields_present() {
        let (_rt, handle) = make_runtime();
        let client = make_exec_client(0);
        let identity = HostGitIdentity {
            name: Some(String::from("Alice")),
            email: Some(String::from("alice@example.com")),
        };

        let result = configure_git_identity(&handle, &client, "c2", &identity)
            .expect("should succeed with Configured");

        assert!(
            matches!(result, GitIdentityResult::Configured { .. }),
            "Expected Configured, got {result:?}"
        );
        if let GitIdentityResult::Configured { name, email } = result {
            assert_eq!(name, "Alice");
            assert_eq!(email, "alice@example.com");
        }
    }

    #[test]
    fn returns_partial_when_only_name_present() {
        let (_rt, handle) = make_runtime();
        let client = make_exec_client(0);
        let identity = HostGitIdentity {
            name: Some(String::from("Bob")),
            email: None,
        };

        let result = configure_git_identity(&handle, &client, "c3", &identity)
            .expect("should succeed with Partial");

        assert!(
            matches!(result, GitIdentityResult::Partial { .. }),
            "Expected Partial, got {result:?}"
        );
        if let GitIdentityResult::Partial {
            name,
            email,
            warnings,
        } = result
        {
            assert_eq!(name.as_deref(), Some("Bob"));
            assert!(email.is_none());
            assert!(warnings.iter().any(|w| w.contains("user.email")));
        }
    }

    #[test]
    fn returns_partial_when_only_email_present() {
        let (_rt, handle) = make_runtime();
        let client = make_exec_client(0);
        let identity = HostGitIdentity {
            name: None,
            email: Some(String::from("carol@example.com")),
        };

        let result = configure_git_identity(&handle, &client, "c4", &identity)
            .expect("should succeed with Partial");

        assert!(
            matches!(result, GitIdentityResult::Partial { .. }),
            "Expected Partial, got {result:?}"
        );
        if let GitIdentityResult::Partial {
            name,
            email,
            warnings,
        } = result
        {
            assert!(name.is_none());
            assert_eq!(email.as_deref(), Some("carol@example.com"));
            assert!(warnings.iter().any(|w| w.contains("user.name")));
        }
    }

    #[test]
    fn propagates_exec_failure_as_error() {
        let (_rt, handle) = make_runtime();
        let client = make_exec_client(1);
        let identity = HostGitIdentity {
            name: Some(String::from("Alice")),
            email: Some(String::from("alice@example.com")),
        };

        let result = configure_git_identity(&handle, &client, "c5", &identity);

        match result {
            Err(PodbotError::Container(crate::error::ContainerError::ExecFailed { .. })) => {}
            other => panic!("Expected Err(ExecFailed), got {other:?}"),
        }
    }
}
