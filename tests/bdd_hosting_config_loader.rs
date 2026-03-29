//! Behavioural tests for hosted library configuration loading.

mod bdd_hosting_config_loader_helpers;

pub use bdd_hosting_config_loader_helpers::{
    HostingConfigLoaderState, hosting_config_loader_state,
};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/hosting_config_loader.feature",
    name = "Hosted configuration loads through the library API"
)]
fn hosted_configuration_loads_through_the_library_api(
    hosting_config_loader_state: HostingConfigLoaderState,
) {
    let _ = hosting_config_loader_state;
}

#[scenario(
    path = "tests/features/hosting_config_loader.feature",
    name = "Hosting env vars override defaults"
)]
fn hosting_env_vars_override_defaults(hosting_config_loader_state: HostingConfigLoaderState) {
    let _ = hosting_config_loader_state;
}

#[scenario(
    path = "tests/features/hosting_config_loader.feature",
    name = "Run intent rejects hosted library configuration"
)]
fn run_intent_rejects_hosted_library_configuration(
    hosting_config_loader_state: HostingConfigLoaderState,
) {
    let _ = hosting_config_loader_state;
}
