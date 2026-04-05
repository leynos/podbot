//! Behavioural tests for hosted-era configuration semantics.

mod bdd_hosting_config_helpers;

pub use bdd_hosting_config_helpers::{HostingConfigState, hosting_config_state};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/hosting_configuration.feature",
    name = "Legacy interactive configuration remains valid"
)]
fn legacy_interactive_configuration_remains_valid(hosting_config_state: HostingConfigState) {
    let _ = hosting_config_state;
}

#[scenario(
    path = "tests/features/hosting_configuration.feature",
    name = "Host-mounted workspace gains a default container path"
)]
fn host_mounted_workspace_gains_a_default_container_path(hosting_config_state: HostingConfigState) {
    let _ = hosting_config_state;
}

#[scenario(
    path = "tests/features/hosting_configuration.feature",
    name = "Run intent rejects hosted modes"
)]
fn run_intent_rejects_hosted_modes(hosting_config_state: HostingConfigState) {
    let _ = hosting_config_state;
}

#[scenario(
    path = "tests/features/hosting_configuration.feature",
    name = "Host mount requires an explicit host path"
)]
fn host_mount_requires_an_explicit_host_path(hosting_config_state: HostingConfigState) {
    let _ = hosting_config_state;
}

#[scenario(
    path = "tests/features/hosting_configuration.feature",
    name = "Custom agents require an explicit command"
)]
fn custom_agents_require_an_explicit_command(hosting_config_state: HostingConfigState) {
    let _ = hosting_config_state;
}
