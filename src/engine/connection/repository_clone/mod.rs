//! Repository cloning inside a running sandbox container.
//!
//! Builds Git commands with credential-free argv and uses `GIT_ASKPASS` to let
//! Git obtain credentials from the mounted helper inside the container.

use crate::api::{AskpassPath, BranchName, RepositoryRef, WorkspacePath};
use crate::engine::{ContainerExecClient, EngineConnector, ExecMode, ExecRequest};
use crate::error::{ContainerError, PodbotError};

/// Request for cloning a repository into a container workspace.
pub struct RepositoryCloneRequest<'a> {
    /// Target container identifier.
    pub container_id: &'a str,
    /// Validated repository coordinates.
    pub repository: &'a RepositoryRef,
    /// Validated target branch.
    pub branch: &'a BranchName,
    /// Validated absolute in-container workspace path.
    pub workspace_base_dir: &'a WorkspacePath,
    /// Validated in-container path to the `GIT_ASKPASS` helper.
    pub askpass_path: &'a AskpassPath,
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
    run_clone(runtime, client, request)?;
    verify_checked_out_branch(runtime, client, request)?;

    Ok(RepositoryCloneResult {
        workspace_path: String::from(request.workspace_base_dir.as_str()),
        checked_out_branch: String::from(request.branch.as_str()),
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
        String::from(request.branch.as_str()),
        String::from("--single-branch"),
        github_remote(request),
        String::from(request.workspace_base_dir.as_str()),
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
        String::from(request.workspace_base_dir.as_str()),
        String::from(request.branch.as_str()),
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
                    format!("GIT_ASKPASS={}", self.request.askpass_path.as_str()),
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
        request.repository.owner(),
        request.repository.name()
    )
}

#[cfg(test)]
mod tests {
    //! Unit tests for container repository-clone command construction.

    use std::io;

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

    fn runtime() -> io::Result<(tokio::runtime::Runtime, tokio::runtime::Handle)> {
        let rt = tokio::runtime::Runtime::new()?;
        let handle = rt.handle().clone();
        Ok((rt, handle))
    }

    fn typed_request_values(
        branch: &str,
    ) -> Result<(RepositoryRef, BranchName, WorkspacePath), PodbotError> {
        let repository = RepositoryRef::parse("leynos/podbot")?;
        let branch_name = BranchName::parse(branch)?;
        let workspace = WorkspacePath::parse("/work")?;
        Ok((repository, branch_name, workspace))
    }

    fn request<'a>(
        repository: &'a RepositoryRef,
        branch_name: &'a BranchName,
        workspace: &'a WorkspacePath,
        askpass: &'a AskpassPath,
    ) -> RepositoryCloneRequest<'a> {
        RepositoryCloneRequest {
            container_id: "sandbox-clone",
            repository,
            branch: branch_name,
            workspace_base_dir: workspace,
            askpass_path: askpass,
        }
    }

    fn typed_askpass() -> Result<AskpassPath, PodbotError> {
        AskpassPath::parse("/usr/local/bin/git-askpass")
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

    fn arrange_successful_clone(client: &mut MockExecClient) {
        expect_exec(
            client,
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
    }

    #[test]
    fn clones_repository_and_verifies_branch() {
        let (_rt, handle) = runtime().expect("test requires a Tokio runtime");
        let mut client = MockExecClient::new();
        let (repository, branch, workspace) =
            typed_request_values("main").expect("test request values should parse");
        let askpass = typed_askpass().expect("test askpass should parse");
        let clone_request = request(&repository, &branch, &workspace, &askpass);
        arrange_successful_clone(&mut client);
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

        let result = clone_repository_into_workspace(&handle, &client, &clone_request)
            .expect("clone should succeed");

        assert_eq!(result.workspace_path, "/work");
        assert_eq!(result.checked_out_branch, "main");
    }

    #[test]
    fn clone_failure_returns_exec_error() {
        let (_rt, handle) = runtime().expect("test requires a Tokio runtime");
        let mut client = MockExecClient::new();
        let (repository, branch, workspace) =
            typed_request_values("main").expect("test request values should parse");
        let askpass = typed_askpass().expect("test askpass should parse");
        let clone_request = request(&repository, &branch, &workspace, &askpass);
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

        let result = clone_repository_into_workspace(&handle, &client, &clone_request);

        assert!(matches!(
            result,
            Err(PodbotError::Container(ContainerError::ExecFailed { .. }))
        ));
    }

    #[test]
    fn branch_verification_failure_returns_exec_error() {
        let (_rt, handle) = runtime().expect("test requires a Tokio runtime");
        let mut client = MockExecClient::new();
        let (repository, branch, workspace) =
            typed_request_values("main").expect("test request values should parse");
        let askpass = typed_askpass().expect("test askpass should parse");
        let clone_request = request(&repository, &branch, &workspace, &askpass);
        arrange_successful_clone(&mut client);
        // Branch verification fails (exit code 1).
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
            1,
        );

        let result = clone_repository_into_workspace(&handle, &client, &clone_request);

        assert!(
            matches!(
                result,
                Err(PodbotError::Container(ContainerError::ExecFailed { .. }))
            ),
            "expected ExecFailed on branch verification failure, got {result:?}"
        );
    }
}
