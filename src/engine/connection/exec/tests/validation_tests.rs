//! Validation-focused tests for exec request construction.

use rstest::rstest;

use super::*;
use crate::error::{ConfigError, PodbotError};

fn assert_exec_request_validation_error(
    result: Result<ExecRequest, PodbotError>,
    expected_field: &str,
) {
    let field = match result {
        Err(PodbotError::Config(
            ConfigError::MissingRequired { field } | ConfigError::InvalidValue { field, .. },
        )) => field,
        other => panic!("expected validation error for '{expected_field}', got {other:?}"),
    };
    assert_eq!(
        field, expected_field,
        "expected validation error for '{expected_field}', got field '{field}'"
    );
}

#[rstest]
fn exec_request_rejects_empty_command() {
    let result = ExecRequest::new("sandbox", vec![], ExecMode::Attached);
    assert_exec_request_validation_error(result, "command");
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
    assert_exec_request_validation_error(result, "container");
}
