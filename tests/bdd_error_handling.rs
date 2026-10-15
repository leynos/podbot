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

/// Marks the point at which the caller inspects the operation's result.
///
/// Inspection is intentionally inert: the outcome is already recorded in the
/// state by the `Given`, and the assertion lives in `the outcome is ok`. The
/// step exists so the Gherkin narrative retains its `When` phase, and the sole
/// scenario using it does assert afterwards.
#[when("the result is inspected")]
fn result_is_inspected(error_state: &ErrorState) {
    let _ = error_state;
}

#[when("the error is formatted")]
fn error_is_formatted(error_state: &ErrorState) -> Result<(), String> {
    let error = error_state
        .error
        .get()
        .ok_or_else(|| String::from("error should be set"))?;
    error_state.message.set(error.to_error().to_string());
    Ok(())
}

#[when("the error is reported")]
fn error_is_reported(error_state: &ErrorState) -> Result<(), String> {
    let error = error_state
        .error
        .get()
        .ok_or_else(|| String::from("error should be set"))?;
    let report = Report::from(error.to_error());
    error_state.message.set(report.to_string());
    Ok(())
}

#[then("the outcome is ok")]
fn outcome_is_ok(error_state: &ErrorState) -> Result<(), String> {
    let success = error_state
        .success
        .get()
        .ok_or_else(|| String::from("success should be set"))?;
    if success {
        Ok(())
    } else {
        Err(String::from("expected the operation to succeed"))
    }
}

fn assert_message_is(error_state: &ErrorState, expected: &str) -> Result<(), String> {
    let message = error_state
        .message
        .get()
        .ok_or_else(|| String::from("message should be set"))?;
    if message == expected {
        Ok(())
    } else {
        Err(format!("Expected message {expected:?}, got: {message:?}"))
    }
}

#[then("the error message is {expected}")]
fn error_message_is(error_state: &ErrorState, expected: String) -> Result<(), String> {
    assert_message_is(error_state, &expected)
}

#[then("the report message is {expected}")]
fn report_message_is(error_state: &ErrorState, expected: String) -> Result<(), String> {
    assert_message_is(error_state, &expected)
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

#[cfg(test)]
mod tests {
    //! Regression tests for missing error state and mismatched verdicts.

    use rstest::rstest;

    use super::{
        ErrorState, assert_message_is, error_is_formatted, error_is_reported, outcome_is_ok,
    };

    #[rstest]
    #[case::format_error(error_is_formatted, "error should be set")]
    #[case::report_error(error_is_reported, "error should be set")]
    #[case::inspect_outcome(outcome_is_ok, "success should be set")]
    fn missing_state_returns_a_setup_error(
        #[case] step: fn(&ErrorState) -> Result<(), String>,
        #[case] expected: &str,
    ) {
        let state = ErrorState::default();
        let result = step(&state);
        let error = result.expect_err("missing scenario state must return an error");
        assert_eq!(
            error, expected,
            "missing state should identify the unset slot"
        );
    }

    #[rstest]
    fn missing_message_returns_a_setup_error() {
        let state = ErrorState::default();
        let result = assert_message_is(&state, "expected");
        let error = result.expect_err("missing message must return an error");
        assert_eq!(
            error, "message should be set",
            "lookup should identify the missing message"
        );
    }

    #[rstest]
    fn mismatched_message_returns_a_verdict_error() {
        let state = ErrorState::default();
        state.message.set(String::from("actual"));
        let result = assert_message_is(&state, "expected");
        let error = result.expect_err("a mismatched message must fail the verdict");
        assert_eq!(
            error, "Expected message \"expected\", got: \"actual\"",
            "a mismatched message should report both values",
        );
    }

    #[rstest]
    fn unsuccessful_outcome_returns_a_verdict_error() {
        let state = ErrorState::default();
        state.success.set(false);
        let result = outcome_is_ok(&state);
        let error = result.expect_err("an unsuccessful outcome must fail the verdict");
        assert_eq!(
            error, "expected the operation to succeed",
            "a failed outcome should explain the verdict",
        );
    }
}
