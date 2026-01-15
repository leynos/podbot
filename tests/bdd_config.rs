//! Behavioural tests for podbot configuration.
//!
//! These tests validate the configuration loading and default behaviour using
//! rstest-bdd.

// Test-specific lint exceptions: expect is standard practice in tests
#![expect(clippy::expect_used, reason = "expect is standard practice in tests")]

use camino::Utf8PathBuf;
use podbot::config::{AgentKind, AppConfig, GitHubConfig, SandboxConfig};
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then};

/// State shared across configuration test scenarios.
#[derive(Default, ScenarioState)]
struct ConfigState {
    /// The loaded application configuration.
    config: Slot<AppConfig>,
    /// The captured configuration parsing error.
    parse_error: Slot<String>,
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

#[given("a configuration file with an invalid agent kind")]
fn config_with_invalid_agent_kind(config_state: &ConfigState) {
    let toml = r#"
        [agent]
        kind = "unknown"
    "#;
    let error = toml::from_str::<AppConfig>(toml)
        .expect_err("TOML parsing should fail for an invalid agent kind");
    config_state.parse_error.set(error.to_string());
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

#[then("the configuration load fails")]
fn configuration_load_fails(config_state: &ConfigState) {
    let error = config_state
        .parse_error
        .get()
        .expect("parse error should be set");
    assert!(
        error.contains("unknown variant"),
        "Expected unknown-variant error, got: {error}"
    );
}

// GitHub configuration validation step definitions

#[given("a complete GitHub configuration")]
fn complete_github_configuration(config_state: &ConfigState) {
    let config = AppConfig {
        github: GitHubConfig {
            app_id: Some(12345),
            installation_id: Some(67890),
            private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
        },
        ..Default::default()
    };
    config_state.config.set(config);
}

#[given("a GitHub configuration missing the app ID")]
fn github_config_missing_app_id(config_state: &ConfigState) {
    let config = AppConfig {
        github: GitHubConfig {
            app_id: None,
            installation_id: Some(67890),
            private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
        },
        ..Default::default()
    };
    config_state.config.set(config);
}

#[given("a GitHub configuration with no fields set")]
fn github_config_all_fields_missing(config_state: &ConfigState) {
    config_state.config.set(AppConfig::default());
}

#[then("GitHub validation succeeds")]
fn github_validation_succeeds(config_state: &ConfigState) {
    let config = config_state
        .config
        .get()
        .expect("configuration should be set");
    let result = config.github.validate();
    assert!(
        result.is_ok(),
        "Expected GitHub validation to succeed: {result:?}"
    );
}

#[then("GitHub validation fails")]
fn github_validation_fails(config_state: &ConfigState) {
    let config = config_state
        .config
        .get()
        .expect("configuration should be set");
    let result = config.github.validate();
    assert!(result.is_err(), "Expected GitHub validation to fail");
    let error = result.expect_err("validation should fail");
    config_state.parse_error.set(error.to_string());
}

#[then("the validation error mentions \"github.app_id\"")]
fn validation_error_mentions_app_id(config_state: &ConfigState) {
    let error = config_state
        .parse_error
        .get()
        .expect("parse error should be set");
    assert!(
        error.contains("github.app_id"),
        "Expected error to mention 'github.app_id', got: {error}"
    );
}

#[then("the validation error mentions all missing GitHub fields")]
fn validation_error_mentions_all_github_fields(config_state: &ConfigState) {
    let error = config_state
        .parse_error
        .get()
        .expect("parse error should be set");
    assert!(
        error.contains("github.app_id"),
        "Error should mention app_id: {error}"
    );
    assert!(
        error.contains("github.installation_id"),
        "Error should mention installation_id: {error}"
    );
    assert!(
        error.contains("github.private_key_path"),
        "Error should mention private_key_path: {error}"
    );
}

#[then("the configuration loads successfully")]
fn configuration_loads_successfully(config_state: &ConfigState) {
    assert!(
        config_state.config.get().is_some(),
        "Configuration should be set"
    );
}

#[then("GitHub is not configured")]
fn github_is_not_configured(config_state: &ConfigState) {
    let config = config_state
        .config
        .get()
        .expect("configuration should be set");
    assert!(
        !config.github.is_configured(),
        "GitHub should not be configured"
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

#[scenario(
    path = "tests/features/configuration.feature",
    name = "Invalid agent kind is rejected"
)]
fn invalid_agent_kind_is_rejected(config_state: ConfigState) {
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
