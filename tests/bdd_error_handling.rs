//! Behavioural tests for podbot error handling.
//!
//! These tests validate user-visible error messages using rstest-bdd.

use eyre::Report;
use podbot::error::{ConfigError, ContainerError, GitHubError, PodbotError};
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};

#[derive(Clone, Debug)]
enum ErrorSnapshot {
    MissingRequired { field: String },
    ContainerStartFailed { container: String, message: String },
    TokenExpired,
}

impl ErrorSnapshot {
    fn to_error(&self) -> PodbotError {
        match self {
            Self::MissingRequired { field } => PodbotError::from(ConfigError::MissingRequired {
                field: field.clone(),
            }),
            Self::ContainerStartFailed { container, message } => {
                PodbotError::from(ContainerError::StartFailed {
                    container_id: container.clone(),
                    message: message.clone(),
                })
            }
            Self::TokenExpired => PodbotError::from(GitHubError::TokenExpired),
        }
    }
}

/// State shared across error handling scenarios.
#[derive(Default, ScenarioState)]
struct ErrorState {
    /// The last error captured during a scenario.
    error: Slot<ErrorSnapshot>,
    /// The formatted error message.
    message: Slot<String>,
    /// Whether the operation succeeded.
    success: Slot<bool>,
}

/// Fixture providing a fresh error state.
#[fixture]
fn error_state() -> ErrorState {
    ErrorState::default()
}

#[given("a successful operation")]
fn successful_operation(error_state: &ErrorState) {
    error_state.success.set(true);
}

#[given("a missing configuration field {field}")]
fn missing_configuration_field(error_state: &ErrorState, field: String) {
    error_state
        .error
        .set(ErrorSnapshot::MissingRequired { field });
    error_state.success.set(false);
}

#[given("a container start failure for {container} with message {message}")]
fn container_start_failure(error_state: &ErrorState, container: String, message: String) {
    error_state
        .error
        .set(ErrorSnapshot::ContainerStartFailed { container, message });
    error_state.success.set(false);
}

#[given("an expired GitHub token")]
fn expired_github_token(error_state: &ErrorState) {
    error_state.error.set(ErrorSnapshot::TokenExpired);
    error_state.success.set(false);
}

#[when("the result is inspected")]
fn result_is_inspected(error_state: &ErrorState) {
    let _ = error_state;
}

#[when("the error is formatted")]
#[expect(
    clippy::expect_used,
    reason = "test assertion - panic on missing state is intentional"
)]
fn error_is_formatted(error_state: &ErrorState) {
    let error = error_state.error.get().expect("error should be set");
    error_state.message.set(error.to_error().to_string());
}

#[when("the error is reported")]
#[expect(
    clippy::expect_used,
    reason = "test assertion - panic on missing state is intentional"
)]
fn error_is_reported(error_state: &ErrorState) {
    let error = error_state.error.get().expect("error should be set");
    let report = Report::from(error.to_error());
    error_state.message.set(report.to_string());
}

#[then("the outcome is ok")]
#[expect(
    clippy::expect_used,
    reason = "test assertion - panic on missing state is intentional"
)]
fn outcome_is_ok(error_state: &ErrorState) {
    let Some(success) = error_state.success.get() else {
        panic!("success should be set");
    };
    assert!(success, "expected the operation to succeed");
}

#[expect(
    clippy::expect_used,
    reason = "test assertion - panic on missing state is intentional"
)]
fn assert_message_is(error_state: &ErrorState, expected: &str) {
    let message = error_state.message.get().expect("message should be set");
    assert_eq!(message, expected);
}

#[then("the error message is {expected}")]
fn error_message_is(error_state: &ErrorState, expected: String) {
    assert_message_is(error_state, &expected);
}

#[then("the report message is {expected}")]
fn report_message_is(error_state: &ErrorState, expected: String) {
    assert_message_is(error_state, &expected);
}

#[scenario(
    path = "tests/features/error_handling.feature",
    name = "Successful operations return ok"
)]
fn successful_operations_return_ok(error_state: ErrorState) {
    let _ = error_state;
}

#[scenario(
    path = "tests/features/error_handling.feature",
    name = "Missing configuration is reported clearly"
)]
fn missing_configuration_is_reported(error_state: ErrorState) {
    let _ = error_state;
}

#[scenario(
    path = "tests/features/error_handling.feature",
    name = "Missing configuration is reported via eyre"
)]
fn missing_configuration_is_reported_via_eyre(error_state: ErrorState) {
    let _ = error_state;
}

#[scenario(
    path = "tests/features/error_handling.feature",
    name = "Container start failures include identifiers"
)]
fn container_start_failures_include_ids(error_state: ErrorState) {
    let _ = error_state;
}

#[scenario(
    path = "tests/features/error_handling.feature",
    name = "Token expiry is reported via eyre"
)]
fn token_expiry_is_reported_via_eyre(error_state: ErrorState) {
    let _ = error_state;
}
