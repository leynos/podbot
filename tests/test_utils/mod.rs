//! Shared test utilities for integration tests.
//!
//! This module shares exec-client mock scaffolding and the API-to-engine exec
//! bridge used by integration tests to inspect command outcomes.

#![cfg(feature = "internal")]

use podbot::api::{CommandOutcome, ExecRequest, exec_with_client_for_tests};
use podbot::engine::ContainerExecClient;
use rstest::rstest;

/// Shared exec-client mock scaffolding for the behavioural suites.
///
/// The module only defines a `#[macro_export]` macro, so it is reachable from
/// every test crate that declares `mod test_utils;` regardless of declaration
/// order.
mod exec_client_mock;

/// Execute an API `ExecRequest` against a mock engine client in integration tests.
///
/// This keeps the engine-to-API translation in one place for integration tests
/// that must stay outside the crate-private `podbot::api::exec_with_client`
/// seam.
///
/// # Errors
///
/// Returns any validation or engine-exec error produced while translating the
/// API request into an engine request and running it through the provided mock
/// client.
pub fn exec_outcome_with_client<C: ContainerExecClient + Sync>(
    client: &C,
    runtime: &tokio::runtime::Handle,
    request: &ExecRequest,
) -> podbot::error::Result<CommandOutcome> {
    exec_with_client_for_tests(client, runtime, request)
}

#[cfg(test)]
mod tests {
    //! Unit tests for the shared integration-test utilities.

    use bollard::exec::{CreateExecOptions, CreateExecResults, StartExecOptions, StartExecResults};
    use bollard::models::ExecInspectResponse;
    use mockall::mock;

    use super::*;
    use podbot::api::ExecMode;
    use podbot::engine::{CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture};

    mock! {
        #[derive(Debug)]
        TestExecClient {}

        impl ContainerExecClient for TestExecClient {
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
                options: bollard::exec::ResizeExecOptions,
            ) -> ResizeExecFuture<'_>;
        }
    }

    #[rstest]
    #[case(0, CommandOutcome::Success)]
    #[case(42, CommandOutcome::CommandExit { code: 42 })]
    fn exec_outcome_with_client_maps_exit_codes(
        #[case] exit_code: i64,
        #[case] expected: CommandOutcome,
    ) {
        let runtime = tokio::runtime::Runtime::new().expect("runtime should be created");
        let request = ExecRequest::new("sandbox", vec![String::from("echo"), String::from("ok")])
            .expect("request should be valid")
            .with_mode(ExecMode::Detached);
        let mut client = MockTestExecClient::new();

        client.expect_create_exec().times(1).returning(|_, _| {
            Box::pin(async {
                Ok(CreateExecResults {
                    id: String::from("test-exec-id"),
                })
            })
        });
        client
            .expect_start_exec()
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(StartExecResults::Detached) }));
        client.expect_inspect_exec().times(1).returning(move |_| {
            let inspect = ExecInspectResponse {
                running: Some(false),
                exit_code: Some(exit_code),
                ..ExecInspectResponse::default()
            };
            Box::pin(async move { Ok(inspect) })
        });
        client.expect_resize_exec().never();

        let result = exec_outcome_with_client(&client, runtime.handle(), &request)
            .expect("exec should succeed");

        assert_eq!(result, expected);
    }
}
