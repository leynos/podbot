//! Behavioural tests for podbot configuration.
//!
//! These tests validate the configuration loading and default behaviour using
//! rstest-bdd.

mod bdd_config_helpers;

pub use bdd_config_helpers::{ConfigState, config_state};
use rstest_bdd_macros::scenario;

// Scenario bindings - each binds a feature scenario to its step implementations

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Default configuration values"
)]
fn default_configuration_values(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Configuration file overrides defaults"
)]
fn configuration_file_overrides_defaults(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Missing optional configuration is acceptable"
)]
fn missing_optional_configuration_acceptable(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Invalid agent kind is rejected"
)]
fn invalid_agent_kind_is_rejected(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Invalid agent mode is rejected"
)]
fn invalid_agent_mode_is_rejected(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "GitHub configuration validates successfully when complete"
)]
fn github_config_validates_when_complete(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "GitHub configuration validation fails when app ID is missing"
)]
fn github_config_fails_when_app_id_missing(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "GitHub configuration validation fails when all fields missing"
)]
fn github_config_fails_when_all_fields_missing(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "GitHub configuration is not required for non-GitHub operations"
)]
fn github_config_not_required_for_non_github_ops(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Sandbox configuration with dev/fuse disabled"
)]
fn sandbox_config_with_dev_fuse_disabled(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Sandbox configuration in minimal mode"
)]
fn sandbox_config_in_minimal_mode(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Sandbox configuration in privileged mode with all options"
)]
fn sandbox_config_privileged_with_all_options(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Workspace configuration overrides the base directory"
)]
fn workspace_config_overrides_base_dir(config_state: ConfigState) {
    let _ = config_state;
}

// Layer precedence scenarios

#[scenario(
    path = "tests/features/configuration.feature",
    name = "File layer overrides defaults"
)]
fn file_layer_overrides_defaults(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Environment layer overrides file layer"
)]
fn env_layer_overrides_file(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "CLI layer overrides all other layers"
)]
fn cli_layer_overrides_all(config_state: ConfigState) {
    let _ = config_state;
}

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Lower layer values are preserved when not overridden"
)]
fn lower_layer_values_preserved(config_state: ConfigState) {
    let _ = config_state;
}
