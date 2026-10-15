//! Regression tests for missing configuration scenario state.

use rstest::rstest;

use super::{
    ConfigState, capture_parse_error, config_with_invalid_agent_kind,
    config_with_invalid_agent_mode, get_config, github_validation_fails,
};

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

#[rstest]
#[case::invalid_kind(config_with_invalid_agent_kind)]
#[case::invalid_mode(config_with_invalid_agent_mode)]
fn invalid_agent_setup_captures_rejection(#[case] setup: fn(&ConfigState) -> Result<(), String>) {
    let state = ConfigState::default();
    state.parse_error.set(String::from("previous rejection"));
    let result = setup(&state);
    result.expect("invalid agent setup should capture a parsing rejection");
    let error = state
        .parse_error
        .get()
        .expect("the rejection should be recorded");
    assert!(
        error.contains("unknown variant `unknown`"),
        "the captured error should explain the invalid agent value: {error}",
    );
    assert!(
        state.config.get().is_none(),
        "invalid setup must not store configuration"
    );
}

#[rstest]
#[case::unset_error(None)]
#[case::previous_error(Some("previous rejection"))]
fn unexpected_parse_success_preserves_state(#[case] previous_error: Option<&str>) {
    let state = ConfigState::default();
    if let Some(message) = previous_error {
        state.parse_error.set(String::from(message));
    }
    let result = capture_parse_error(&state, "", "invalid agent setup unexpectedly succeeded");
    let error = result.expect_err("valid TOML must fail invalid-agent setup");
    assert_eq!(
        error, "invalid agent setup unexpectedly succeeded",
        "unexpected success should preserve the caller's setup context",
    );
    assert_eq!(
        state.parse_error.get().as_deref(),
        previous_error,
        "unexpected success must not overwrite the error slot",
    );
    assert!(
        state.config.get().is_none(),
        "setup must not store configuration"
    );
}
