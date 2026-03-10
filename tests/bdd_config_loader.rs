//! Behavioural tests for the library-facing configuration loader.
//!
//! These scenarios exercise the `podbot::config` loader APIs directly, without
//! going through Clap parsing.

mod bdd_config_loader_helpers;

pub use bdd_config_loader_helpers::{ConfigLoaderState, config_loader_state};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/config_loader.feature",
    name = "Load defaults when no sources are provided"
)]
fn load_defaults_when_no_sources_are_provided(config_loader_state: ConfigLoaderState) {
    let _ = config_loader_state;
}

#[scenario(
    path = "tests/features/config_loader.feature",
    name = "Load configuration from an explicit path hint"
)]
fn load_configuration_from_an_explicit_path_hint(config_loader_state: ConfigLoaderState) {
    let _ = config_loader_state;
}

#[scenario(
    path = "tests/features/config_loader.feature",
    name = "Environment overrides configuration file"
)]
fn environment_overrides_configuration_file(config_loader_state: ConfigLoaderState) {
    let _ = config_loader_state;
}

#[scenario(
    path = "tests/features/config_loader.feature",
    name = "Host overrides take precedence over environment and file"
)]
fn host_overrides_take_precedence_over_environment_and_file(
    config_loader_state: ConfigLoaderState,
) {
    let _ = config_loader_state;
}

#[scenario(
    path = "tests/features/config_loader.feature",
    name = "Invalid typed environment values fail fast"
)]
fn invalid_typed_environment_values_fail_fast(config_loader_state: ConfigLoaderState) {
    let _ = config_loader_state;
}
