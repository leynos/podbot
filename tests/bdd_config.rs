//! Behavioural tests for podbot configuration.
//!
//! These tests validate the configuration loading and default behaviour using
//! rstest-bdd.

// Test-specific lint exceptions: expect is standard practice in tests
#![expect(clippy::expect_used, reason = "expect is standard practice in tests")]

use camino::Utf8PathBuf;
use podbot::config::{
    AgentKind, AgentMode, AppConfig, GitHubConfig, SandboxConfig, WorkspaceConfig,
};
use podbot::error::{ConfigError, PodbotError};
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then};

// Helper functions to reduce duplication in step definitions

/// Extracts the configuration from state with consistent error handling.
fn get_config(config_state: &ConfigState) -> AppConfig {
    config_state
        .config
        .get()
        .expect("configuration should be set")
}

/// Creates and sets an `AppConfig` with a custom `GitHubConfig`.
fn set_github_config(
    config_state: &ConfigState,
    app_id: Option<u64>,
    installation_id: Option<u64>,
    private_key_path: Option<&str>,
) {
    let config = AppConfig {
        github: GitHubConfig {
            app_id,
            installation_id,
            private_key_path: private_key_path.map(Utf8PathBuf::from),
        },
        ..Default::default()
    };
    config_state.config.set(config);
}

/// Asserts that an optional GitHub field is absent.
fn assert_github_field_absent<T>(field: Option<&T>, field_name: &str) {
    assert!(field.is_none(), "Expected {field_name} to be absent");
}

/// Creates and sets an `AppConfig` with a custom `SandboxConfig`.
fn set_sandbox_config(config_state: &ConfigState, privileged: bool, mount_dev_fuse: bool) {
    let config = AppConfig {
        sandbox: SandboxConfig {
            privileged,
            mount_dev_fuse,
        },
        ..Default::default()
    };
    config_state.config.set(config);
}

/// Creates and sets an `AppConfig` with a custom workspace base directory.
fn set_workspace_base_dir(config_state: &ConfigState, base_dir: &str) {
    let config = AppConfig {
        workspace: WorkspaceConfig {
            base_dir: Utf8PathBuf::from(base_dir),
        },
        ..Default::default()
    };
    config_state.config.set(config);
}

/// State shared across configuration test scenarios.
#[derive(Default, ScenarioState)]
struct ConfigState {
    /// The loaded application configuration.
    config: Slot<AppConfig>,
    /// The captured configuration parsing error (for TOML parse errors).
    parse_error: Slot<String>,
    /// The captured missing field names from validation errors.
    missing_fields: Slot<String>,
}

/// Fixture providing a fresh configuration state.
#[fixture]
fn config_state() -> ConfigState {
    ConfigState::default()
}

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

#[given("a configuration file with an invalid agent mode")]
fn config_with_invalid_agent_mode(config_state: &ConfigState) {
    let toml = r#"
        [agent]
        mode = "unknown"
    "#;
    let error = toml::from_str::<AppConfig>(toml)
        .expect_err("TOML parsing should fail for an invalid agent mode");
    config_state.parse_error.set(error.to_string());
}

#[given("a configuration file with workspace base directory set to {base_dir}")]
fn config_with_workspace_base_dir(config_state: &ConfigState, base_dir: String) {
    set_workspace_base_dir(config_state, &base_dir);
}

#[then("the sandbox is not privileged")]
fn sandbox_is_not_privileged(config_state: &ConfigState) {
    let config = get_config(config_state);
    assert!(
        !config.sandbox.privileged,
        "Expected sandbox to not be privileged"
    );
}

#[then("the sandbox is privileged")]
fn sandbox_is_privileged(config_state: &ConfigState) {
    let config = get_config(config_state);
    assert!(
        config.sandbox.privileged,
        "Expected sandbox to be privileged"
    );
}

#[then("dev/fuse mounting is enabled")]
fn dev_fuse_mounting_enabled(config_state: &ConfigState) {
    let config = get_config(config_state);
    assert!(
        config.sandbox.mount_dev_fuse,
        "Expected dev/fuse mounting to be enabled"
    );
}

#[then("the agent kind is Claude")]
fn agent_kind_is_claude(config_state: &ConfigState) {
    let config = get_config(config_state);
    assert_eq!(
        config.agent.kind,
        AgentKind::Claude,
        "Expected agent kind to be Claude"
    );
}

#[then("the agent mode is podbot")]
fn agent_mode_is_podbot(config_state: &ConfigState) {
    let config = get_config(config_state);
    assert_eq!(
        config.agent.mode,
        AgentMode::Podbot,
        "Expected agent mode to be podbot"
    );
}

#[then("the workspace base directory is {base_dir}")]
fn workspace_base_dir_is(config_state: &ConfigState, base_dir: String) {
    let config = get_config(config_state);
    assert_eq!(
        config.workspace.base_dir.as_str(),
        base_dir.as_str(),
        "Expected workspace base directory to be {}",
        base_dir
    );
}

#[then("the app ID is absent")]
fn app_id_is_absent(config_state: &ConfigState) {
    let config = get_config(config_state);
    assert_github_field_absent(config.github.app_id.as_ref(), "app ID");
}

#[then("the installation ID is absent")]
fn installation_id_is_absent(config_state: &ConfigState) {
    let config = get_config(config_state);
    assert_github_field_absent(config.github.installation_id.as_ref(), "installation ID");
}

#[then("the private key path is absent")]
fn private_key_path_is_absent(config_state: &ConfigState) {
    let config = get_config(config_state);
    assert_github_field_absent(config.github.private_key_path.as_ref(), "private key path");
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
    set_github_config(
        config_state,
        Some(12345),
        Some(67890),
        Some("/path/to/key.pem"),
    );
}

#[given("a GitHub configuration missing the app ID")]
fn github_config_missing_app_id(config_state: &ConfigState) {
    set_github_config(config_state, None, Some(67890), Some("/path/to/key.pem"));
}

#[given("a GitHub configuration with no fields set")]
fn github_config_all_fields_missing(config_state: &ConfigState) {
    set_github_config(config_state, None, None, None);
}

#[then("GitHub validation succeeds")]
fn github_validation_succeeds(config_state: &ConfigState) {
    let config = get_config(config_state);
    let result = config.github.validate();
    assert!(
        result.is_ok(),
        "Expected GitHub validation to succeed: {result:?}"
    );
}

#[then("GitHub validation fails")]
fn github_validation_fails(config_state: &ConfigState) {
    let config = get_config(config_state);
    let result = config.github.validate();
    assert!(result.is_err(), "Expected GitHub validation to fail");
    let error = result.expect_err("validation should fail");
    // Extract the field value from the error variant rather than relying on Display
    match error {
        PodbotError::Config(ConfigError::MissingRequired { field }) => {
            config_state.missing_fields.set(field);
        }
        other => panic!("Expected ConfigError::MissingRequired, got: {other:?}"),
    }
}

#[then("the validation error mentions \"github.app_id\"")]
fn validation_error_mentions_app_id(config_state: &ConfigState) {
    let missing = config_state
        .missing_fields
        .get()
        .expect("missing fields should be set");
    assert!(
        missing.contains("github.app_id"),
        "Expected missing fields to contain 'github.app_id', got: {missing}"
    );
}

#[then("the validation error mentions all missing GitHub fields")]
fn validation_error_mentions_all_github_fields(config_state: &ConfigState) {
    let missing = config_state
        .missing_fields
        .get()
        .expect("missing fields should be set");
    assert!(
        missing.contains("github.app_id"),
        "Missing fields should contain app_id: {missing}"
    );
    assert!(
        missing.contains("github.installation_id"),
        "Missing fields should contain installation_id: {missing}"
    );
    assert!(
        missing.contains("github.private_key_path"),
        "Missing fields should contain private_key_path: {missing}"
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
    let config = get_config(config_state);
    assert!(
        !config.github.is_configured(),
        "GitHub should not be configured"
    );
}

// Sandbox configuration step definitions

#[given("a configuration file with dev/fuse mounting disabled")]
fn config_with_dev_fuse_disabled(config_state: &ConfigState) {
    set_sandbox_config(config_state, false, false);
}

#[given("a configuration file in minimal mode")]
fn config_in_minimal_mode(config_state: &ConfigState) {
    set_sandbox_config(config_state, false, true);
}

#[given("a configuration file with privileged mode and dev/fuse disabled")]
fn config_with_privileged_and_no_fuse(config_state: &ConfigState) {
    set_sandbox_config(config_state, true, false);
}

#[then("dev/fuse mounting is disabled")]
fn dev_fuse_mounting_disabled(config_state: &ConfigState) {
    let config = get_config(config_state);
    assert!(
        !config.sandbox.mount_dev_fuse,
        "Expected dev/fuse mounting to be disabled"
    );
}

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
