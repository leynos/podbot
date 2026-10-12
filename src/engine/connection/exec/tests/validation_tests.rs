//! Validation-focused tests for exec request construction.

use rstest::rstest;

use super::*;
use crate::error::{ConfigError, PodbotError};

/// Extracts the offending field name from a configuration validation error.
///
/// Returns the debug rendering of the actual result in the error case so the
/// caller can report the mismatch; this helper never panics.
fn validation_error_field(result: Result<ExecRequest, PodbotError>) -> Result<String, String> {
    match result {
        Err(PodbotError::Config(
            ConfigError::MissingRequired { field } | ConfigError::InvalidValue { field, .. },
        )) => Ok(field),
        other => Err(format!("{other:?}")),
    }
}

/// Asserts that request construction failed validation on `$expected_field`.
///
/// A macro rather than a function so failures report the calling test's line
/// number.
macro_rules! assert_validation_error_field {
    ($result:expr, $expected_field:expr $(,)?) => {
        match validation_error_field($result) {
            Ok(field) => assert_eq!(
                field, $expected_field,
                "expected validation error for '{}', got field '{}'",
                $expected_field, field
            ),
            Err(actual) => panic!(
                "expected validation error for '{}', got {}",
                $expected_field, actual
            ),
        }
    };
}

#[rstest]
fn exec_request_rejects_empty_command() {
    let result = ExecRequest::new("sandbox", vec![], ExecMode::Attached);
    assert_validation_error_field!(result, "command");
}

#[rstest]
#[case(vec![String::new()])]
#[case(vec![String::from("   "), String::from("echo")])]
fn exec_request_rejects_blank_executable_entry(#[case] command: Vec<String>) {
    let result = ExecRequest::new("sandbox", command, ExecMode::Attached);
    assert!(
        matches!(
            result,
            Err(PodbotError::Config(ConfigError::InvalidValue { ref field, .. }))
                if field == "command"
        ),
        "expected invalid executable error, got {result:?}"
    );
}

#[rstest]
#[case(vec![String::from("echo"), String::new()])]
#[case(vec![String::from("echo"), String::from("   ")])]
fn exec_request_allows_blank_non_executable_entries(#[case] command: Vec<String>) {
    let expected = command.clone();
    let request = ExecRequest::new("sandbox", command, ExecMode::Attached)
        .expect("command with blank non-executable arguments should be accepted");
    assert_eq!(request.command(), expected.as_slice());
}

#[rstest]
fn exec_request_rejects_blank_container_id() {
    let result = ExecRequest::new("   ", vec![String::from("echo")], ExecMode::Detached);
    assert_validation_error_field!(result, "container");
}
