//! Behavioural tests for podbot configuration.
//!
//! These tests validate the configuration loading and default behaviour using
//! rstest-bdd.

// Test-specific lint exceptions: expect is standard practice in tests
#![expect(clippy::expect_used, reason = "expect is standard practice in tests")]

use podbot::config::{AgentKind, AppConfig, SandboxConfig};
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then};

/// State shared across configuration test scenarios.
#[derive(Default, ScenarioState)]
struct ConfigState {
    /// The loaded application configuration.
    config: Slot<AppConfig>,
}

/// Fixture providing a fresh configuration state.
#[fixture]
fn config_state() -> ConfigState {
    ConfigState::default()
}

// Step definitions

#[given("no configuration is provided")]
fn no_configuration_provided(config_state: &ConfigState) {
    config_state.config.set(AppConfig::default());
}

#[given("a configuration file with privileged mode enabled")]
fn config_with_privileged_mode(config_state: &ConfigState) {
    let config = AppConfig {
        sandbox: SandboxConfig {
            privileged: true,
            mount_dev_fuse: true,
        },
        ..Default::default()
    };
    config_state.config.set(config);
}

#[given("no GitHub configuration is provided")]
fn no_github_configuration(config_state: &ConfigState) {
    // Default config has no GitHub settings
    config_state.config.set(AppConfig::default());
}

#[then("the sandbox is not privileged")]
fn sandbox_is_not_privileged(config_state: &ConfigState) {
    let config = config_state
        .config
        .get()
        .expect("configuration should be set");
    assert!(
        !config.sandbox.privileged,
        "Expected sandbox to not be privileged"
    );
}

#[then("the sandbox is privileged")]
fn sandbox_is_privileged(config_state: &ConfigState) {
    let config = config_state
        .config
        .get()
        .expect("configuration should be set");
    assert!(
        config.sandbox.privileged,
        "Expected sandbox to be privileged"
    );
}

#[then("dev/fuse mounting is enabled")]
fn dev_fuse_mounting_enabled(config_state: &ConfigState) {
    let config = config_state
        .config
        .get()
        .expect("configuration should be set");
    assert!(
        config.sandbox.mount_dev_fuse,
        "Expected dev/fuse mounting to be enabled"
    );
}

#[then("the agent kind is Claude")]
fn agent_kind_is_claude(config_state: &ConfigState) {
    let config = config_state
        .config
        .get()
        .expect("configuration should be set");
    assert_eq!(
        config.agent.kind,
        AgentKind::Claude,
        "Expected agent kind to be Claude"
    );
}

#[then("the workspace base directory is /work")]
fn workspace_base_dir_is_work(config_state: &ConfigState) {
    let config = config_state
        .config
        .get()
        .expect("configuration should be set");
    assert_eq!(
        config.workspace.base_dir.as_str(),
        "/work",
        "Expected workspace base directory to be /work"
    );
}

#[then("the app ID is absent")]
fn app_id_is_absent(config_state: &ConfigState) {
    let config = config_state
        .config
        .get()
        .expect("configuration should be set");
    assert!(
        config.github.app_id.is_none(),
        "Expected app ID to be absent"
    );
}

#[then("the installation ID is absent")]
fn installation_id_is_absent(config_state: &ConfigState) {
    let config = config_state
        .config
        .get()
        .expect("configuration should be set");
    assert!(
        config.github.installation_id.is_none(),
        "Expected installation ID to be absent"
    );
}

#[then("the private key path is absent")]
fn private_key_path_is_absent(config_state: &ConfigState) {
    let config = config_state
        .config
        .get()
        .expect("configuration should be set");
    assert!(
        config.github.private_key_path.is_none(),
        "Expected private key path to be absent"
    );
}

// Scenario bindings

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
