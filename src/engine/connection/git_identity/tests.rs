//! Unit tests for Git identity reading and container configuration.

use std::sync::{Arc, Mutex};

use bollard::exec::{
    CreateExecOptions, CreateExecResults, ResizeExecOptions, StartExecOptions, StartExecResults,
};
use bollard::models::ExecInspectResponse;
use rstest::{fixture, rstest};

use super::*;
use crate::engine::{
    ContainerExecClient, CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture,
};

// =============================================================================
// Mock GitIdentityReader
// =============================================================================

/// Deterministic reader that returns pre-configured identity values.
struct MockGitIdentityReader {
    identity: GitIdentity,
}

impl MockGitIdentityReader {
    fn new(name: Option<&str>, email: Option<&str>) -> Self {
        Self {
            identity: GitIdentity::new(name.map(String::from), email.map(String::from)),
        }
    }
}

impl GitIdentityReader for MockGitIdentityReader {
    fn read_git_identity(&self) -> GitIdentity {
        self.identity.clone()
    }
}

// =============================================================================
// Mock ContainerExecClient
// =============================================================================

/// Recorded exec invocation for assertion.
#[derive(Debug, Clone)]
struct ExecCall {
    container_id: String,
    cmd: Vec<String>,
}

/// Mock exec client that records calls and returns configurable results.
struct MockExecClient {
    calls: Arc<Mutex<Vec<ExecCall>>>,
    create_fail: bool,
    start_fail: bool,
    inspect_fail: bool,
    exit_code: i64,
}

impl MockExecClient {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            create_fail: false,
            start_fail: false,
            inspect_fail: false,
            exit_code: 0,
        }
    }

    fn with_create_failure(mut self) -> Self {
        self.create_fail = true;
        self
    }

    fn with_start_failure(mut self) -> Self {
        self.start_fail = true;
        self
    }

    fn with_inspect_failure(mut self) -> Self {
        self.inspect_fail = true;
        self
    }

    fn with_exit_code(mut self, code: i64) -> Self {
        self.exit_code = code;
        self
    }

    fn calls(&self) -> Vec<ExecCall> {
        self.calls.lock().expect("lock poisoned").clone()
    }
}

impl ContainerExecClient for MockExecClient {
    fn create_exec(
        &self,
        container_id: &str,
        options: CreateExecOptions<String>,
    ) -> CreateExecFuture<'_> {
        let cid = String::from(container_id);
        let cmd = options.cmd.clone().unwrap_or_default();
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(ExecCall {
                container_id: cid,
                cmd,
            });

        let fail = self.create_fail;
        Box::pin(async move {
            if fail {
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 500,
                    message: String::from("mock create failure"),
                })
            } else {
                Ok(CreateExecResults {
                    id: String::from("mock-exec-id"),
                })
            }
        })
    }

    fn start_exec(
        &self,
        _exec_id: &str,
        _options: Option<StartExecOptions>,
    ) -> StartExecFuture<'_> {
        let fail = self.start_fail;
        Box::pin(async move {
            if fail {
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 500,
                    message: String::from("mock start failure"),
                })
            } else {
                Ok(StartExecResults::Detached)
            }
        })
    }

    fn inspect_exec(&self, _exec_id: &str) -> InspectExecFuture<'_> {
        let fail = self.inspect_fail;
        let exit_code = self.exit_code;
        Box::pin(async move {
            if fail {
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 500,
                    message: String::from("mock inspect failure"),
                })
            } else {
                Ok(ExecInspectResponse {
                    running: Some(false),
                    exit_code: Some(exit_code),
                    ..ExecInspectResponse::default()
                })
            }
        })
    }

    fn resize_exec(
        &self,
        _exec_id: &str,
        _options: ResizeExecOptions,
    ) -> ResizeExecFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

// =============================================================================
// Fixtures
// =============================================================================

#[fixture]
fn container_id() -> &'static str {
    "test-container-abc123"
}

#[fixture]
fn mock_client() -> MockExecClient {
    MockExecClient::new()
}

// =============================================================================
// GitIdentity tests
// =============================================================================

#[rstest]
fn identity_with_both_fields_is_complete() {
    let identity = GitIdentity::new(
        Some(String::from("Alice")),
        Some(String::from("alice@example.com")),
    );
    assert!(identity.is_complete());
    assert!(!identity.is_empty());
    assert!(identity.has_name());
    assert!(identity.has_email());
    assert_eq!(identity.name(), Some("Alice"));
    assert_eq!(identity.email(), Some("alice@example.com"));
}

#[rstest]
fn identity_with_name_only_is_partial() {
    let identity = GitIdentity::new(Some(String::from("Alice")), None);
    assert!(!identity.is_complete());
    assert!(!identity.is_empty());
    assert!(identity.has_name());
    assert!(!identity.has_email());
}

#[rstest]
fn identity_with_email_only_is_partial() {
    let identity = GitIdentity::new(None, Some(String::from("alice@example.com")));
    assert!(!identity.is_complete());
    assert!(!identity.is_empty());
    assert!(!identity.has_name());
    assert!(identity.has_email());
}

#[rstest]
fn empty_identity_reports_correctly() {
    let identity = GitIdentity::default();
    assert!(identity.is_empty());
    assert!(!identity.is_complete());
    assert!(!identity.has_name());
    assert!(!identity.has_email());
    assert_eq!(identity.name(), None);
    assert_eq!(identity.email(), None);
}

// =============================================================================
// MockGitIdentityReader tests
// =============================================================================

#[rstest]
fn mock_reader_returns_configured_identity() {
    let reader = MockGitIdentityReader::new(Some("Bob"), Some("bob@example.com"));
    let identity = reader.read_git_identity();
    assert_eq!(identity.name(), Some("Bob"));
    assert_eq!(identity.email(), Some("bob@example.com"));
}

#[rstest]
fn mock_reader_returns_empty_identity() {
    let reader = MockGitIdentityReader::new(None, None);
    let identity = reader.read_git_identity();
    assert!(identity.is_empty());
}

// =============================================================================
// configure_git_identity_async tests
// =============================================================================

#[rstest]
#[tokio::test]
async fn apply_both_fields_executes_two_config_commands(
    container_id: &str,
    mock_client: MockExecClient,
) {
    let identity = GitIdentity::new(
        Some(String::from("Alice")),
        Some(String::from("alice@example.com")),
    );
    let result = EngineConnector::apply_git_identity_async(
        &mock_client,
        container_id,
        &identity,
    )
    .await
    .expect("should succeed");

    assert!(result.name_applied());
    assert!(result.email_applied());
    assert!(result.warnings().is_empty());

    let calls = mock_client.calls();
    assert_eq!(calls.len(), 2, "expected two exec calls");
    assert_eq!(
        calls[0].cmd,
        vec!["git", "config", "--global", "user.name", "Alice"]
    );
    assert_eq!(
        calls[1].cmd,
        vec!["git", "config", "--global", "user.email", "alice@example.com"]
    );
}

#[rstest]
#[tokio::test]
async fn apply_name_only_warns_about_missing_email(
    container_id: &str,
    mock_client: MockExecClient,
) {
    let identity = GitIdentity::new(Some(String::from("Alice")), None);
    let result = EngineConnector::apply_git_identity_async(
        &mock_client,
        container_id,
        &identity,
    )
    .await
    .expect("should succeed");

    assert!(result.name_applied());
    assert!(!result.email_applied());
    assert_eq!(result.warnings().len(), 1);
    assert!(result.warnings()[0].contains("user.email"));

    let calls = mock_client.calls();
    assert_eq!(calls.len(), 1, "expected one exec call for name only");
}

#[rstest]
#[tokio::test]
async fn apply_email_only_warns_about_missing_name(
    container_id: &str,
    mock_client: MockExecClient,
) {
    let identity = GitIdentity::new(None, Some(String::from("alice@example.com")));
    let result = EngineConnector::apply_git_identity_async(
        &mock_client,
        container_id,
        &identity,
    )
    .await
    .expect("should succeed");

    assert!(!result.name_applied());
    assert!(result.email_applied());
    assert_eq!(result.warnings().len(), 1);
    assert!(result.warnings()[0].contains("user.name"));

    let calls = mock_client.calls();
    assert_eq!(calls.len(), 1, "expected one exec call for email only");
}

#[rstest]
#[tokio::test]
async fn apply_empty_identity_warns_and_makes_no_exec_calls(
    container_id: &str,
    mock_client: MockExecClient,
) {
    let identity = GitIdentity::default();
    let result = EngineConnector::apply_git_identity_async(
        &mock_client,
        container_id,
        &identity,
    )
    .await
    .expect("should succeed");

    assert!(!result.name_applied());
    assert!(!result.email_applied());
    assert!(result.is_empty());
    assert_eq!(result.warnings().len(), 1);
    assert!(result.warnings()[0].contains("no Git identity configured"));

    let calls = mock_client.calls();
    assert!(calls.is_empty(), "expected no exec calls for empty identity");
}

#[rstest]
#[tokio::test]
async fn configure_reads_identity_and_applies_it(container_id: &str) {
    let client = MockExecClient::new();
    let reader = MockGitIdentityReader::new(Some("Carol"), Some("carol@example.com"));

    let result = EngineConnector::configure_git_identity_async(
        &client,
        container_id,
        &reader,
    )
    .await
    .expect("should succeed");

    assert!(result.name_applied());
    assert!(result.email_applied());
    assert!(result.warnings().is_empty());
}

#[rstest]
#[tokio::test]
async fn configure_with_empty_reader_warns(container_id: &str) {
    let client = MockExecClient::new();
    let reader = MockGitIdentityReader::new(None, None);

    let result = EngineConnector::configure_git_identity_async(
        &client,
        container_id,
        &reader,
    )
    .await
    .expect("should succeed");

    assert!(result.is_empty());
    assert!(!result.warnings().is_empty());
}

#[rstest]
#[tokio::test]
async fn empty_container_id_returns_error() {
    let client = MockExecClient::new();
    let reader = MockGitIdentityReader::new(Some("Alice"), Some("alice@example.com"));

    let err = EngineConnector::configure_git_identity_async(&client, "", &reader)
        .await
        .expect_err("should fail with empty container ID");

    assert!(err.to_string().contains("container ID must not be empty"));
}

#[rstest]
#[tokio::test]
async fn whitespace_container_id_returns_error() {
    let client = MockExecClient::new();
    let identity = GitIdentity::new(
        Some(String::from("Alice")),
        Some(String::from("alice@example.com")),
    );

    let err = EngineConnector::apply_git_identity_async(&client, "   ", &identity)
        .await
        .expect_err("should fail with whitespace container ID");

    assert!(err.to_string().contains("container ID must not be empty"));
}

#[rstest]
#[tokio::test]
async fn create_exec_failure_produces_warning_not_error(container_id: &str) {
    let client = MockExecClient::new().with_create_failure();
    let identity = GitIdentity::new(
        Some(String::from("Alice")),
        Some(String::from("alice@example.com")),
    );

    let result = EngineConnector::apply_git_identity_async(
        &client,
        container_id,
        &identity,
    )
    .await
    .expect("should succeed despite exec failures");

    assert!(!result.name_applied());
    assert!(!result.email_applied());
    assert!(result.warnings().len() >= 2);
    assert!(result.warnings()[0].contains("failed to create exec"));
}

#[rstest]
#[tokio::test]
async fn start_exec_failure_produces_warning_not_error(container_id: &str) {
    let client = MockExecClient::new().with_start_failure();
    let identity = GitIdentity::new(
        Some(String::from("Alice")),
        Some(String::from("alice@example.com")),
    );

    let result = EngineConnector::apply_git_identity_async(
        &client,
        container_id,
        &identity,
    )
    .await
    .expect("should succeed despite exec failures");

    assert!(!result.name_applied());
    assert!(!result.email_applied());
    assert!(result.warnings().len() >= 2);
    assert!(result.warnings()[0].contains("failed to start exec"));
}

#[rstest]
#[tokio::test]
async fn inspect_exec_failure_produces_warning_not_error(container_id: &str) {
    let client = MockExecClient::new().with_inspect_failure();
    let identity = GitIdentity::new(
        Some(String::from("Alice")),
        Some(String::from("alice@example.com")),
    );

    let result = EngineConnector::apply_git_identity_async(
        &client,
        container_id,
        &identity,
    )
    .await
    .expect("should succeed despite inspect failures");

    assert!(!result.name_applied());
    assert!(!result.email_applied());
    assert!(result.warnings().len() >= 2);
    assert!(result.warnings()[0].contains("failed to inspect exec"));
}

#[rstest]
#[tokio::test]
async fn nonzero_exit_code_produces_warning_not_error(container_id: &str) {
    let client = MockExecClient::new().with_exit_code(1);
    let identity = GitIdentity::new(
        Some(String::from("Alice")),
        Some(String::from("alice@example.com")),
    );

    let result = EngineConnector::apply_git_identity_async(
        &client,
        container_id,
        &identity,
    )
    .await
    .expect("should succeed despite non-zero exit code");

    assert!(!result.name_applied());
    assert!(!result.email_applied());
    assert!(result.warnings().len() >= 2);
    assert!(result.warnings()[0].contains("exited with code 1"));
}

#[rstest]
#[tokio::test]
async fn container_id_is_passed_to_exec_calls(mock_client: MockExecClient) {
    let identity = GitIdentity::new(
        Some(String::from("Alice")),
        Some(String::from("alice@example.com")),
    );

    let _ = EngineConnector::apply_git_identity_async(
        &mock_client,
        "my-special-container",
        &identity,
    )
    .await;

    let calls = mock_client.calls();
    for call in &calls {
        assert_eq!(call.container_id, "my-special-container");
    }
}
