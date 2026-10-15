//! Regression tests for missing configuration scenario state.

use rstest::rstest;

use super::{ConfigState, get_config, github_validation_fails};

#[rstest]
fn missing_configuration_returns_a_setup_error() {
    let state = ConfigState::default();
    let result = get_config(&state);
    let error = result.expect_err("an unset configuration must fail lookup");
    assert_eq!(
        error, "configuration should be set",
        "lookup should identify the missing configuration",
    );
}

#[rstest]
fn validation_propagates_missing_configuration() {
    let state = ConfigState::default();
    let result = github_validation_fails(&state);
    let error = result.expect_err("validation must propagate missing configuration");
    assert_eq!(
        error, "configuration should be set",
        "lookup should identify the missing configuration",
    );
}
