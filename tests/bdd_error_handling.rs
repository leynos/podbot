//! Behavioural tests for podbot error handling.
//!
//! These tests validate user-visible error messages using rstest-bdd.

#![expect(clippy::expect_used, reason = "expect is standard practice in tests")]

use std::sync::Arc;

use podbot::error::{ConfigError, ContainerError, PodbotError};
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};

/// State shared across error handling scenarios.
#[derive(Default, ScenarioState)]
struct ErrorState {
    /// The last error captured during a scenario.
    error: Slot<Arc<PodbotError>>,
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
    let error = ConfigError::MissingRequired { field };
    error_state.error.set(Arc::new(PodbotError::from(error)));
    error_state.success.set(false);
}

#[given("a container start failure for {container} with message {message}")]
fn container_start_failure(error_state: &ErrorState, container: String, message: String) {
    let error = ContainerError::StartFailed {
        container_id: container,
        message,
    };
    error_state.error.set(Arc::new(PodbotError::from(error)));
    error_state.success.set(false);
}

#[when("the result is inspected")]
fn result_is_inspected(error_state: &ErrorState) {
    let _ = error_state;
}

#[when("the error is formatted")]
fn error_is_formatted(error_state: &ErrorState) {
    let error = error_state.error.get().expect("error should be set");
    error_state.message.set(error.as_ref().to_string());
}

#[then("the outcome is ok")]
fn outcome_is_ok(error_state: &ErrorState) {
    let success = error_state.success.get().expect("success should be set");
    assert!(success, "expected the operation to succeed");
}

#[then("the error message is {expected}")]
fn error_message_is(error_state: &ErrorState, expected: String) {
    let message = error_state.message.get().expect("message should be set");
    assert_eq!(message, expected);
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
    name = "Container start failures include identifiers"
)]
fn container_start_failures_include_ids(error_state: ErrorState) {
    let _ = error_state;
}
