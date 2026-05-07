//! Repository cloning inside a running sandbox container.
//!
//! Builds Git commands with credential-free argv and uses `GIT_ASKPASS` to let
//! Git obtain credentials from the mounted helper inside the container.

use crate::engine::{ContainerExecClient, EngineConnector, ExecMode, ExecRequest};
use crate::error::{ConfigError, ContainerError, PodbotError};

/// Request for cloning a repository into a container workspace.
pub struct RepositoryCloneRequest<'a> {
    /// Target container identifier.
    pub container_id: &'a str,
    /// Repository owner segment.
    pub repository_owner: &'a str,
    /// Repository name segment.
    pub repository_name: &'a str,
    /// Required branch.
    pub branch: &'a str,
    /// Exact destination path inside the container.
    pub workspace_base_dir: &'a str,
    /// Path to the in-container `GIT_ASKPASS` helper.
    pub askpass_path: &'a str,
}

/// Successful repository clone result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryCloneResult {
    /// Exact workspace path used as the clone destination.
    pub workspace_path: String,
    /// Branch verified as checked out after cloning.
    pub checked_out_branch: String,
}

/// Clone a GitHub repository into the requested workspace path.
///
/// # Errors
///
/// Returns validation errors for missing paths and `ContainerError::ExecFailed`
/// when either clone or branch verification fails in the container.
pub fn clone_repository_into_workspace<C: ContainerExecClient + Sync>(
    runtime: &tokio::runtime::Handle,
    client: &C,
    request: &RepositoryCloneRequest<'_>,
) -> Result<RepositoryCloneResult, PodbotError> {
    validate_non_empty("workspace.base_dir", request.workspace_base_dir)?;
    validate_non_empty("git.askpass_path", request.askpass_path)?;

    run_clone(runtime, client, request)?;
    verify_checked_out_branch(runtime, client, request)?;

    Ok(RepositoryCloneResult {
        workspace_path: String::from(request.workspace_base_dir),
        checked_out_branch: String::from(request.branch),
    })
}

fn run_clone<C: ContainerExecClient + Sync>(
    runtime: &tokio::runtime::Handle,
    client: &C,
    request: &RepositoryCloneRequest<'_>,
) -> Result<(), PodbotError> {
    let runner = GitCommandRunner::new(runtime, client, request);
    let command = vec![
        String::from("git"),
        String::from("clone"),
        String::from("--branch"),
        String::from(request.branch),
        String::from("--single-branch"),
        github_remote(request),
        String::from(request.workspace_base_dir),
    ];
    run_git_command(&runner, command, "git clone")
}

fn verify_checked_out_branch<C: ContainerExecClient + Sync>(
    runtime: &tokio::runtime::Handle,
    client: &C,
    request: &RepositoryCloneRequest<'_>,
) -> Result<(), PodbotError> {
    let runner = GitCommandRunner::new(runtime, client, request);
    let command = vec![
        String::from("sh"),
        String::from("-c"),
        String::from(r#"test "$(git -C "$1" rev-parse --abbrev-ref HEAD)" = "$2""#),
        String::from("podbot-verify-branch"),
        String::from(request.workspace_base_dir),
        String::from(request.branch),
    ];
    run_git_command(&runner, command, "branch verification")
}

fn run_git_command<C: ContainerExecClient + Sync>(
    runner: &GitCommandRunner<'_, C>,
    command: Vec<String>,
    label: &str,
) -> Result<(), PodbotError> {
    runner.run(command, label)
}

struct GitCommandRunner<'a, C: ContainerExecClient + Sync> {
    runtime: &'a tokio::runtime::Handle,
    client: &'a C,
    request: &'a RepositoryCloneRequest<'a>,
}

impl<'a, C: ContainerExecClient + Sync> GitCommandRunner<'a, C> {
    const fn new(
        runtime: &'a tokio::runtime::Handle,
        client: &'a C,
        request: &'a RepositoryCloneRequest<'a>,
    ) -> Self {
        Self {
            runtime,
            client,
            request,
        }
    }

    fn run(&self, command: Vec<String>, label: &str) -> Result<(), PodbotError> {
        let exec_request =
            ExecRequest::new(self.request.container_id, command, ExecMode::Detached)?.with_env(
                Some(vec![
                    format!("GIT_ASKPASS={}", self.request.askpass_path),
                    String::from("GIT_TERMINAL_PROMPT=0"),
                ]),
            );
        let result = EngineConnector::exec(self.runtime, self.client, &exec_request)?;

        if result.exit_code() != 0 {
            return Err(ContainerError::ExecFailed {
                container_id: String::from(self.request.container_id),
                message: format!("{label} failed with exit code {}", result.exit_code()),
            }
            .into());
        }

        Ok(())
    }
}

fn github_remote(request: &RepositoryCloneRequest<'_>) -> String {
    format!(
        "https://github.com/{}/{}.git",
        request.repository_owner, request.repository_name
    )
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), PodbotError> {
    if value.trim().is_empty() {
        return Err(ConfigError::MissingRequired {
            field: String::from(field),
        }
        .into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture};
    use bollard::exec::{CreateExecOptions, ResizeExecOptions, StartExecOptions};
    use mockall::{mock, predicate::eq};

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

    fn runtime() -> (tokio::runtime::Runtime, tokio::runtime::Handle) {
        let rt = tokio::runtime::Runtime::new().expect("test requires a Tokio runtime");
        let handle = rt.handle().clone();
        (rt, handle)
    }

    fn request(branch: &str) -> RepositoryCloneRequest<'_> {
        RepositoryCloneRequest {
            container_id: "sandbox-clone",
            repository_owner: "leynos",
            repository_name: "podbot",
            branch,
            workspace_base_dir: "/work",
            askpass_path: "/usr/local/bin/git-askpass",
        }
    }

    fn expect_exec(client: &mut MockExecClient, command: Vec<&'static str>, exit_code: i64) {
        let expected: Vec<String> = command.into_iter().map(String::from).collect();
        client
            .expect_create_exec()
            .withf(move |container_id, options| {
                container_id == "sandbox-clone"
                    && options.cmd.as_ref() == Some(&expected)
                    && options.env.as_ref().is_some_and(|env| {
                        env == &vec![
                            String::from("GIT_ASKPASS=/usr/local/bin/git-askpass"),
                            String::from("GIT_TERMINAL_PROMPT=0"),
                        ]
                    })
            })
            .times(1)
            .returning(|_, _| {
                Box::pin(async {
                    Ok(bollard::exec::CreateExecResults {
                        id: String::from("exec-id"),
                    })
                })
            });
        client
            .expect_start_exec()
            .with(
                eq("exec-id"),
                eq(Some(StartExecOptions {
                    detach: true,
                    tty: false,
                    output_capacity: None,
                })),
            )
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) }));
        client.expect_inspect_exec().times(1).returning(move |_| {
            Box::pin(async move {
                Ok(bollard::models::ExecInspectResponse {
                    exit_code: Some(exit_code),
                    running: Some(false),
                    ..Default::default()
                })
            })
        });
    }

    #[test]
    fn clones_repository_and_verifies_branch() {
        let (_rt, handle) = runtime();
        let mut client = MockExecClient::new();
        expect_exec(
            &mut client,
            vec![
                "git",
                "clone",
                "--branch",
                "main",
                "--single-branch",
                "https://github.com/leynos/podbot.git",
                "/work",
            ],
            0,
        );
        expect_exec(
            &mut client,
            vec![
                "sh",
                "-c",
                r#"test "$(git -C "$1" rev-parse --abbrev-ref HEAD)" = "$2""#,
                "podbot-verify-branch",
                "/work",
                "main",
            ],
            0,
        );

        let result = clone_repository_into_workspace(&handle, &client, &request("main"))
            .expect("clone should succeed");

        assert_eq!(result.workspace_path, "/work");
        assert_eq!(result.checked_out_branch, "main");
    }

    #[test]
    fn clone_failure_returns_exec_error() {
        let (_rt, handle) = runtime();
        let mut client = MockExecClient::new();
        expect_exec(
            &mut client,
            vec![
                "git",
                "clone",
                "--branch",
                "main",
                "--single-branch",
                "https://github.com/leynos/podbot.git",
                "/work",
            ],
            128,
        );

        let result = clone_repository_into_workspace(&handle, &client, &request("main"));

        assert!(matches!(
            result,
            Err(PodbotError::Container(ContainerError::ExecFailed { .. }))
        ));
    }
}
