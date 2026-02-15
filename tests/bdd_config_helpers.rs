//! Behavioural test helpers for podbot configuration.

use camino::Utf8PathBuf;
use ortho_config::MergeComposer;
use ortho_config::serde_json::json;
use podbot::config::{
    AgentKind, AgentMode, AppConfig, GitHubConfig, SandboxConfig, WorkspaceConfig,
};
use podbot::error::{ConfigError, PodbotError};
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, then, when};

// Helper functions to reduce duplication in step definitions

/// Extracts the configuration from state with consistent error handling.
#[expect(clippy::expect_used, reason = "test helper - panics are acceptable")]
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
            ..Default::default()
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
pub struct ConfigState {
    /// The loaded application configuration.
    config: Slot<AppConfig>,
    /// The captured configuration parsing error (for TOML parse errors).
    parse_error: Slot<String>,
    /// The captured missing field names from validation errors.
    missing_fields: Slot<String>,
    /// File layer JSON value for layer precedence tests.
    file_layer: Slot<ortho_config::serde_json::Value>,
    /// Environment layer JSON value for layer precedence tests.
    env_layer: Slot<ortho_config::serde_json::Value>,
    /// CLI layer JSON value for layer precedence tests.
    cli_layer: Slot<ortho_config::serde_json::Value>,
}

/// Fixture providing a fresh configuration state.
#[fixture]
pub fn config_state() -> ConfigState {
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
            ..Default::default()
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
#[expect(clippy::expect_used, reason = "test step - panics are acceptable")]
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
#[expect(clippy::expect_used, reason = "test step - panics are acceptable")]
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
#[expect(clippy::expect_used, reason = "test step - panics are acceptable")]
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
#[expect(clippy::expect_used, reason = "test step - panics are acceptable")]
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
#[expect(clippy::expect_used, reason = "test step - panics are acceptable")]
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
#[expect(clippy::expect_used, reason = "test step - panics are acceptable")]
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

// Layer precedence step definitions

/// Recursively merges two JSON values, combining nested objects field-by-field.
///
/// For object values, fields are merged recursively. For non-objects, the new
/// value completely overwrites the existing one. This mirrors how `OrthoConfig`
/// merges nested configuration structures.
fn merge_json_values(
    existing: &ortho_config::serde_json::Value,
    new_value: &ortho_config::serde_json::Value,
) -> ortho_config::serde_json::Value {
    use ortho_config::serde_json::Value;

    match (existing, new_value) {
        (Value::Object(existing_obj), Value::Object(new_obj)) => {
            let mut merged = existing_obj.clone();
            for (key, new_child) in new_obj {
                if let Some(existing_child) = merged.get(key) {
                    // Recursively merge nested objects; for non-objects the new value wins.
                    merged.insert(key.clone(), merge_json_values(existing_child, new_child));
                } else {
                    merged.insert(key.clone(), new_child.clone());
                }
            }
            Value::Object(merged)
        }
        // For non-object values, the new value completely overwrites the existing one.
        _ => new_value.clone(),
    }
}

/// Merges a new value into an existing layer slot (if present).
fn merge_layer(
    existing: Option<ortho_config::serde_json::Value>,
    new_value: ortho_config::serde_json::Value,
) -> ortho_config::serde_json::Value {
    if let Some(existing_value) = existing {
        merge_json_values(&existing_value, &new_value)
    } else {
        new_value
    }
}

/// Merges the current file layer with a new value (combining fields).
fn merge_file_layer(
    config_state: &ConfigState,
    new_value: ortho_config::serde_json::Value,
) -> ortho_config::serde_json::Value {
    merge_layer(config_state.file_layer.get(), new_value)
}

/// Merges the current env layer with a new value (combining fields).
fn merge_env_layer(
    config_state: &ConfigState,
    new_value: ortho_config::serde_json::Value,
) -> ortho_config::serde_json::Value {
    merge_layer(config_state.env_layer.get(), new_value)
}

#[given("defaults provide engine_socket as nil")]
fn defaults_provide_engine_socket_nil(config_state: &ConfigState) {
    // Defaults already have engine_socket as None, nothing to do.
    // Access config_state to satisfy clippy (rstest_bdd requires the parameter).
    drop(config_state.file_layer.get());
}

#[given("a file layer provides engine_socket as {socket}")]
fn file_layer_provides_engine_socket(config_state: &ConfigState, socket: String) {
    let merged = merge_file_layer(config_state, json!({ "engine_socket": socket }));
    config_state.file_layer.set(merged);
}

#[given("an environment layer provides engine_socket as {socket}")]
fn env_layer_provides_engine_socket(config_state: &ConfigState, socket: String) {
    let merged = merge_env_layer(config_state, json!({ "engine_socket": socket }));
    config_state.env_layer.set(merged);
}

#[given("a CLI layer provides engine_socket as {socket}")]
fn cli_layer_provides_engine_socket(config_state: &ConfigState, socket: String) {
    config_state
        .cli_layer
        .set(json!({ "engine_socket": socket }));
}

#[given("a file layer provides image as {image}")]
fn file_layer_provides_image(config_state: &ConfigState, image: String) {
    let merged = merge_file_layer(config_state, json!({ "image": image }));
    config_state.file_layer.set(merged);
}

#[given("a file layer provides sandbox.privileged as {value}")]
fn file_layer_provides_sandbox_privileged(config_state: &ConfigState, value: bool) {
    let merged = merge_file_layer(config_state, json!({ "sandbox": { "privileged": value } }));
    config_state.file_layer.set(merged);
}

#[given("a file layer provides sandbox.mount_dev_fuse as {value}")]
fn file_layer_provides_sandbox_mount_dev_fuse(config_state: &ConfigState, value: bool) {
    let merged = merge_file_layer(
        config_state,
        json!({ "sandbox": { "mount_dev_fuse": value } }),
    );
    config_state.file_layer.set(merged);
}

#[given("an environment layer provides sandbox.privileged as {value}")]
fn env_layer_provides_sandbox_privileged(config_state: &ConfigState, value: bool) {
    let merged = merge_env_layer(config_state, json!({ "sandbox": { "privileged": value } }));
    config_state.env_layer.set(merged);
}

#[when("configuration is merged")]
#[expect(clippy::expect_used, reason = "test step - panics are acceptable")]
fn configuration_is_merged(config_state: &ConfigState) {
    let mut composer = MergeComposer::new();

    // Layer 1: Defaults
    let defaults = ortho_config::serde_json::to_value(AppConfig::default())
        .expect("serialization should succeed");
    composer.push_defaults(defaults);

    // Layer 2: File
    if let Some(file_layer) = config_state.file_layer.get() {
        composer.push_file(file_layer, None);
    }

    // Layer 3: Environment
    if let Some(env_layer) = config_state.env_layer.get() {
        composer.push_environment(env_layer);
    }

    // Layer 4: CLI
    if let Some(cli_layer) = config_state.cli_layer.get() {
        composer.push_cli(cli_layer);
    }

    let config: AppConfig =
        AppConfig::merge_from_layers(composer.layers()).expect("merge should succeed");
    config_state.config.set(config);
}

#[then("the engine socket is {socket}")]
fn engine_socket_is(config_state: &ConfigState, socket: String) {
    let config = get_config(config_state);
    assert_eq!(
        config.engine_socket.as_deref(),
        Some(socket.as_str()),
        "Expected engine socket to be {}",
        socket
    );
}

#[then("the image is {image}")]
fn image_is(config_state: &ConfigState, image: String) {
    let config = get_config(config_state);
    assert_eq!(
        config.image.as_deref(),
        Some(image.as_str()),
        "Expected image to be {}",
        image
    );
}
