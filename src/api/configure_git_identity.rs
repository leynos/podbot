//! Git identity configuration orchestration.
//!
//! Reads Git identity from the host and configures it within the
//! container. Missing identity fields produce warnings rather than
//! errors, following the principle that Git identity is helpful but
//! not required for all container operations.

use crate::engine::{
    ContainerExecClient, GitIdentityResult, HostCommandRunner,
    configure_git_identity as engine_configure, read_host_git_identity,
};
use crate::error::Result as PodbotResult;

/// Parameters for Git identity configuration.
pub struct GitIdentityParams<'a, C: ContainerExecClient + Sync, R: HostCommandRunner> {
    /// Pre-connected container engine client.
    pub client: &'a C,
    /// Host command runner for reading Git config.
    pub host_runner: &'a R,
    /// Target container identifier.
    pub container_id: &'a str,
    /// Tokio runtime handle for blocking execution.
    pub runtime_handle: &'a tokio::runtime::Handle,
}

/// Read host Git identity and configure it in the container.
///
/// This is the top-level orchestration entry point for Step 4.1.
/// Missing host identity fields result in a partial or none-configured
/// result rather than an error.
///
/// # Errors
///
/// Returns `ContainerError::ExecFailed` if a `git config` command
/// fails to execute within the container.
pub fn configure_container_git_identity<C: ContainerExecClient + Sync, R: HostCommandRunner>(
    params: &GitIdentityParams<'_, C, R>,
) -> PodbotResult<GitIdentityResult> {
    let identity = read_host_git_identity(params.host_runner);
    engine_configure(
        params.runtime_handle,
        params.client,
        params.container_id,
        &identity,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::test_helpers::{failure_output, success_output};
    use crate::engine::{CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture};

    use bollard::exec::{CreateExecOptions, ResizeExecOptions, StartExecOptions};
    use mockall::mock;
    use std::io;
    use std::process::Output;

    mock! {
        HostRunner {}
        impl HostCommandRunner for HostRunner {
            fn run_command<'a>(
                &self,
                program: &'a str,
                args: &'a [&'a str],
            ) -> io::Result<Output>;
        }
    }

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

    /// Register a host runner expectation matching `config_key`, returning
    /// `success_output` for `Some` values or `failure_output` for `None`.
    fn register_host_config(
        runner: &mut MockHostRunner,
        config_key: &'static str,
        value: Option<&str>,
    ) {
        match value {
            Some(v) => {
                let owned = String::from(v);
                runner
                    .expect_run_command()
                    .withf(move |_, args| args.contains(&config_key))
                    .returning(move |_, _| Ok(success_output(&format!("{owned}\n"))));
            }
            None => {
                runner
                    .expect_run_command()
                    .withf(move |_, args| args.contains(&config_key))
                    .returning(|_, _| Ok(failure_output()));
            }
        }
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

    /// Shared test plumbing: creates a Tokio runtime, builds
    /// `GitIdentityParams`, and calls `configure_container_git_identity`.
    fn invoke(
        host_runner: &MockHostRunner,
        exec_client: &MockExecClient,
        container_id: &str,
    ) -> PodbotResult<GitIdentityResult> {
        let runtime = tokio::runtime::Runtime::new().expect("test requires a Tokio runtime");
        let handle = runtime.handle().clone();
        let params = GitIdentityParams {
            client: exec_client,
            host_runner,
            container_id,
            runtime_handle: &handle,
        };
        configure_container_git_identity(&params)
    }

    #[test]
    fn returns_configured_when_both_fields_present() {
        let mut host_runner = MockHostRunner::new();
        register_host_config(&mut host_runner, "user.name", Some("Alice"));
        register_host_config(&mut host_runner, "user.email", Some("alice@example.com"));
        let exec_client = make_exec_client(0);

        let result = invoke(&host_runner, &exec_client, "sandbox-unit")
            .expect("should succeed with Configured");

        assert!(
            matches!(result, GitIdentityResult::Configured { .. }),
            "Expected Configured, got {result:?}"
        );
    }

    #[test]
    fn returns_none_configured_when_both_fields_absent() {
        let mut host_runner = MockHostRunner::new();
        register_host_config(&mut host_runner, "user.name", None);
        register_host_config(&mut host_runner, "user.email", None);
        let exec_client = MockExecClient::new(); // no exec calls expected

        let result = invoke(&host_runner, &exec_client, "sandbox-unit-2")
            .expect("should succeed with NoneConfigured");

        assert!(
            matches!(result, GitIdentityResult::NoneConfigured { .. }),
            "Expected NoneConfigured, got {result:?}"
        );
    }
}
