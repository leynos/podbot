//! Unit tests for podbot configuration types.

use super::*;
use camino::Utf8PathBuf;
use ortho_config::MergeComposer;
use ortho_config::serde_json::json;
use rstest::{fixture, rstest};

/// Fixture providing a default `AppConfig`.
#[fixture]
fn app_config() -> AppConfig {
    AppConfig::default()
}

/// Fixture providing a default `SandboxConfig`.
#[fixture]
fn sandbox_config() -> SandboxConfig {
    SandboxConfig::default()
}

/// Fixture providing a default `WorkspaceConfig`.
#[fixture]
fn workspace_config() -> WorkspaceConfig {
    WorkspaceConfig::default()
}

/// Fixture providing a default `CredsConfig`.
#[fixture]
fn creds_config() -> CredsConfig {
    CredsConfig::default()
}

/// Fixture providing an `AppConfig` parsed from a full TOML example.
#[fixture]
fn app_config_from_full_toml() -> AppConfig {
    let toml = r#"
        engine_socket = "unix:///run/podman/podman.sock"
        image = "ghcr.io/example/sandbox:latest"

        [github]
        app_id = 12345
        installation_id = 67890

        [sandbox]
        privileged = true
        mount_dev_fuse = false

        [agent]
        kind = "codex"
        mode = "podbot"

        [workspace]
        base_dir = "/home/user/work"
    "#;

    toml::from_str(toml).expect("TOML parsing should succeed")
}

/// Fixture providing an `AppConfig` parsed from a minimal TOML example.
#[fixture]
fn app_config_from_partial_toml() -> AppConfig {
    let toml = r#"
        engine_socket = "unix:///tmp/docker.sock"
    "#;

    toml::from_str(toml).expect("TOML parsing should succeed")
}

/// Fixture providing a fully configured `GitHubConfig`.
#[fixture]
fn github_config_complete() -> GitHubConfig {
    GitHubConfig {
        app_id: Some(12345),
        installation_id: Some(67890),
        private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
    }
}

/// Helper: Creates a `MergeComposer` with defaults layer already pushed.
fn create_composer_with_defaults() -> MergeComposer {
    let mut composer = MergeComposer::new();
    let defaults = ortho_config::serde_json::to_value(AppConfig::default())
        .expect("serialization should succeed");
    composer.push_defaults(defaults);
    composer
}

/// Helper: Merges layers from a composer into `AppConfig`.
fn merge_config(composer: MergeComposer) -> AppConfig {
    AppConfig::merge_from_layers(composer.layers()).expect("merge should succeed")
}

#[rstest]
fn agent_kind_default_is_claude() {
    assert_eq!(AgentKind::default(), AgentKind::Claude);
}

#[rstest]
fn agent_mode_default_is_podbot() {
    assert_eq!(AgentMode::default(), AgentMode::Podbot);
}

#[rstest]
#[case(AgentKind::Claude, "claude")]
#[case(AgentKind::Codex, "codex")]
fn agent_kind_serialises_to_lowercase(#[case] kind: AgentKind, #[case] expected: &str) {
    let serialised = serde_json::to_string(&kind).expect("serialisation should succeed");
    assert_eq!(serialised, format!("\"{expected}\""));
}

#[rstest]
#[case(AgentMode::Podbot, "podbot")]
fn agent_mode_serialises_to_lowercase(#[case] mode: AgentMode, #[case] expected: &str) {
    let serialised = serde_json::to_string(&mode).expect("serialisation should succeed");
    assert_eq!(serialised, format!("\"{expected}\""));
}

#[rstest]
#[case("\"claude\"", AgentKind::Claude)]
#[case("\"codex\"", AgentKind::Codex)]
fn agent_kind_deserialises_from_lowercase(#[case] input: &str, #[case] expected: AgentKind) {
    let kind: AgentKind = serde_json::from_str(input).expect("deserialisation should succeed");
    assert_eq!(kind, expected);
}

#[rstest]
#[case("\"podbot\"", AgentMode::Podbot)]
fn agent_mode_deserialises_from_lowercase(#[case] input: &str, #[case] expected: AgentMode) {
    let mode: AgentMode = serde_json::from_str(input).expect("deserialisation should succeed");
    assert_eq!(mode, expected);
}

#[rstest]
fn agent_config_defaults_include_mode() {
    let config = AgentConfig::default();
    assert_eq!(config.kind, AgentKind::Claude);
    assert_eq!(config.mode, AgentMode::Podbot);
}

#[rstest]
fn sandbox_config_default_values(sandbox_config: SandboxConfig) {
    assert!(!sandbox_config.privileged);
    assert!(sandbox_config.mount_dev_fuse);
}

#[rstest]
fn workspace_config_default_base_dir(workspace_config: WorkspaceConfig) {
    assert_eq!(workspace_config.base_dir.as_str(), "/work");
}

#[rstest]
fn creds_config_default_copies_both(creds_config: CredsConfig) {
    assert!(creds_config.copy_claude);
    assert!(creds_config.copy_codex);
}

#[rstest]
fn app_config_engine_and_image_default_to_none(app_config: AppConfig) {
    assert!(app_config.engine_socket.is_none());
    assert!(app_config.image.is_none());
}

#[rstest]
fn app_config_sandbox_mount_dev_fuse_defaults_to_true(app_config: AppConfig) {
    assert!(app_config.sandbox.mount_dev_fuse);
}

#[rstest]
fn app_config_sandbox_privileged_defaults_to_false(app_config: AppConfig) {
    assert!(!app_config.sandbox.privileged);
}

#[rstest]
fn app_config_github_defaults_to_none(app_config: AppConfig) {
    assert!(app_config.github.app_id.is_none());
    assert!(app_config.github.installation_id.is_none());
    assert!(app_config.github.private_key_path.is_none());
}

#[rstest]
fn app_config_agent_defaults_to_claude_podbot(app_config: AppConfig) {
    assert_eq!(app_config.agent.kind, AgentKind::Claude);
    assert_eq!(app_config.agent.mode, AgentMode::Podbot);
}

#[rstest]
fn app_config_workspace_defaults_to_work_dir(app_config: AppConfig) {
    assert_eq!(app_config.workspace.base_dir.as_str(), "/work");
}

#[rstest]
fn app_config_toml_sets_engine_socket_and_image(app_config_from_full_toml: AppConfig) {
    assert_eq!(
        app_config_from_full_toml.engine_socket.as_deref(),
        Some("unix:///run/podman/podman.sock")
    );
    assert_eq!(
        app_config_from_full_toml.image.as_deref(),
        Some("ghcr.io/example/sandbox:latest")
    );
}

#[rstest]
fn app_config_toml_sets_github_ids(app_config_from_full_toml: AppConfig) {
    assert_eq!(app_config_from_full_toml.github.app_id, Some(12345));
    assert_eq!(
        app_config_from_full_toml.github.installation_id,
        Some(67890)
    );
}

#[rstest]
fn app_config_toml_sets_sandbox_flags(app_config_from_full_toml: AppConfig) {
    assert!(app_config_from_full_toml.sandbox.privileged);
    assert!(!app_config_from_full_toml.sandbox.mount_dev_fuse);
}

#[rstest]
fn app_config_toml_sets_agent_config(app_config_from_full_toml: AppConfig) {
    assert_eq!(app_config_from_full_toml.agent.kind, AgentKind::Codex);
    assert_eq!(app_config_from_full_toml.agent.mode, AgentMode::Podbot);
}

#[rstest]
fn app_config_toml_sets_workspace_base_dir(app_config_from_full_toml: AppConfig) {
    assert_eq!(
        app_config_from_full_toml.workspace.base_dir.as_str(),
        "/home/user/work"
    );
}

#[rstest]
fn app_config_partial_toml_sets_engine_socket(app_config_from_partial_toml: AppConfig) {
    assert_eq!(
        app_config_from_partial_toml.engine_socket.as_deref(),
        Some("unix:///tmp/docker.sock")
    );
}

#[rstest]
fn app_config_partial_toml_image_defaults_to_none(app_config_from_partial_toml: AppConfig) {
    assert!(app_config_from_partial_toml.image.is_none());
}

#[rstest]
fn app_config_partial_toml_github_app_id_defaults_to_none(app_config_from_partial_toml: AppConfig) {
    assert!(app_config_from_partial_toml.github.app_id.is_none());
}

#[rstest]
fn app_config_partial_toml_sandbox_defaults_apply(app_config_from_partial_toml: AppConfig) {
    assert!(!app_config_from_partial_toml.sandbox.privileged);
    assert!(app_config_from_partial_toml.sandbox.mount_dev_fuse);
}

#[rstest]
fn app_config_partial_toml_agent_defaults_apply(app_config_from_partial_toml: AppConfig) {
    assert_eq!(app_config_from_partial_toml.agent.kind, AgentKind::Claude);
    assert_eq!(app_config_from_partial_toml.agent.mode, AgentMode::Podbot);
}

#[rstest]
fn app_config_partial_toml_workspace_default_applies(app_config_from_partial_toml: AppConfig) {
    assert_eq!(
        app_config_from_partial_toml.workspace.base_dir.as_str(),
        "/work"
    );
}

#[rstest]
#[case("kind", "unknown")]
#[case("mode", "unknown")]
fn app_config_rejects_invalid_agent_field(#[case] field: &str, #[case] value: &str) {
    let toml = format!(
        r#"
        [agent]
        {field} = "{value}"
    "#
    );

    let error = toml::from_str::<AppConfig>(&toml)
        .expect_err("TOML parsing should fail for an invalid agent field");
    assert!(
        error.to_string().contains(value),
        "Expected error mentioning the invalid value \"{value}\", got: {error}"
    );
}

// GitHubConfig validation tests

#[rstest]
fn github_config_validate_succeeds_when_complete(github_config_complete: GitHubConfig) {
    let result = github_config_complete.validate();
    assert!(
        result.is_ok(),
        "Expected validation to succeed for complete config"
    );
}

#[rstest]
#[case(
    None,
    None,
    None,
    "github.app_id, github.installation_id, github.private_key_path"
)]
#[case(
    Some(123),
    None,
    None,
    "github.installation_id, github.private_key_path"
)]
#[case(None, Some(456), None, "github.app_id, github.private_key_path")]
#[case(
    None,
    None,
    Some(Utf8PathBuf::from("/k.pem")),
    "github.app_id, github.installation_id"
)]
#[case(Some(123), Some(456), None, "github.private_key_path")]
#[case(
    Some(123),
    None,
    Some(Utf8PathBuf::from("/k.pem")),
    "github.installation_id"
)]
#[case(None, Some(456), Some(Utf8PathBuf::from("/k.pem")), "github.app_id")]
// Zero values are treated as missing (GitHub never issues ID 0)
#[case(
    Some(0),
    Some(67890),
    Some(Utf8PathBuf::from("/k.pem")),
    "github.app_id"
)]
#[case(
    Some(12345),
    Some(0),
    Some(Utf8PathBuf::from("/k.pem")),
    "github.installation_id"
)]
#[case(
    Some(0),
    Some(0),
    Some(Utf8PathBuf::from("/k.pem")),
    "github.app_id, github.installation_id"
)]
fn github_config_validate_reports_missing_fields(
    #[case] app_id: Option<u64>,
    #[case] installation_id: Option<u64>,
    #[case] private_key_path: Option<Utf8PathBuf>,
    #[case] expected_fields: &str,
) {
    let config = GitHubConfig {
        app_id,
        installation_id,
        private_key_path,
    };
    let result = config.validate();
    let error = result.expect_err("validation should fail with missing fields");
    match error {
        crate::error::PodbotError::Config(crate::error::ConfigError::MissingRequired { field }) => {
            assert_eq!(
                field, expected_fields,
                "Field mismatch: expected '{expected_fields}', got '{field}'"
            );
        }
        other => panic!("Expected ConfigError::MissingRequired, got: {other:?}"),
    }
}

#[rstest]
fn github_config_is_configured_true_when_complete(github_config_complete: GitHubConfig) {
    assert!(github_config_complete.is_configured());
}

#[rstest]
fn github_config_is_configured_false_when_default() {
    let config = GitHubConfig::default();
    assert!(!config.is_configured());
}

#[rstest]
fn github_config_is_configured_false_when_partial() {
    let config = GitHubConfig {
        app_id: Some(12345),
        installation_id: None,
        private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
    };
    assert!(!config.is_configured());
}

#[rstest]
#[case(Some(0), Some(67890))]
#[case(Some(12345), Some(0))]
fn github_config_is_configured_false_when_id_is_zero(
    #[case] app_id: Option<u64>,
    #[case] installation_id: Option<u64>,
) {
    let config = GitHubConfig {
        app_id,
        installation_id,
        private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
    };
    assert!(!config.is_configured());
}

// ============================================================================
// Layer Precedence Tests (MergeComposer)
// ============================================================================

/// Test that serialised `AppConfig::default()` can round-trip through `MergeComposer`.
///
/// This mirrors the production `load_config` behaviour, which serialises
/// `AppConfig::default()` as the defaults layer.
#[rstest]
fn layer_precedence_serialised_defaults_round_trip() {
    // This is exactly what load_config does: serialise defaults, push to composer.
    let composer = create_composer_with_defaults();
    let config = merge_config(composer);
    let expected = AppConfig::default();

    // Verify key fields match to ensure the serialisation round-trip works.
    assert_eq!(config.engine_socket, expected.engine_socket);
    assert_eq!(config.image, expected.image);
    assert_eq!(config.sandbox.privileged, expected.sandbox.privileged);
    assert_eq!(
        config.sandbox.mount_dev_fuse,
        expected.sandbox.mount_dev_fuse
    );
    assert_eq!(config.workspace.base_dir, expected.workspace.base_dir);
    assert_eq!(config.agent.kind, expected.agent.kind);
    assert_eq!(config.agent.mode, expected.agent.mode);
    assert_eq!(config.creds.copy_claude, expected.creds.copy_claude);
    assert_eq!(config.creds.copy_codex, expected.creds.copy_codex);
}

/// Test that defaults layer provides baseline configuration values.
#[rstest]
fn layer_precedence_defaults_provide_baseline() {
    let composer = create_composer_with_defaults();
    let config = merge_config(composer);

    // Defaults should come from serde's #[serde(default)]
    assert!(config.engine_socket.is_none());
    assert!(config.image.is_none());
    assert!(!config.sandbox.privileged);
    assert!(config.sandbox.mount_dev_fuse);
    assert_eq!(config.workspace.base_dir.as_str(), "/work");
}

/// Test that file layer overrides defaults.
#[rstest]
fn layer_precedence_file_overrides_defaults() {
    let mut composer = create_composer_with_defaults();
    composer.push_file(
        json!({
            "engine_socket": "unix:///from/file.sock",
            "image": "file-image:latest"
        }),
        None,
    );

    let config = merge_config(composer);

    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///from/file.sock")
    );
    assert_eq!(config.image.as_deref(), Some("file-image:latest"));
}

/// Test that environment layer overrides file layer.
#[rstest]
fn layer_precedence_env_overrides_file() {
    let mut composer = create_composer_with_defaults();
    composer.push_file(
        json!({
            "engine_socket": "unix:///from/file.sock",
            "image": "file-image:latest"
        }),
        None,
    );
    composer.push_environment(json!({
        "engine_socket": "unix:///from/env.sock"
    }));

    let config = merge_config(composer);

    // Environment overrides file for engine_socket
    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///from/env.sock")
    );
    // File value preserved for image (not in env layer)
    assert_eq!(config.image.as_deref(), Some("file-image:latest"));
}

/// Test that CLI layer overrides all other layers.
#[rstest]
fn layer_precedence_cli_overrides_all() {
    let mut composer = create_composer_with_defaults();
    composer.push_file(
        json!({
            "engine_socket": "unix:///from/file.sock",
            "image": "file-image:latest"
        }),
        None,
    );
    composer.push_environment(json!({
        "engine_socket": "unix:///from/env.sock"
    }));
    composer.push_cli(json!({
        "engine_socket": "unix:///from/cli.sock"
    }));

    let config = merge_config(composer);

    // CLI overrides everything for engine_socket
    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///from/cli.sock")
    );
    // File value preserved for image (not in env or CLI layers)
    assert_eq!(config.image.as_deref(), Some("file-image:latest"));
}

/// Test full precedence chain: defaults < file < env < CLI.
#[rstest]
fn layer_precedence_full_chain() {
    let mut composer = create_composer_with_defaults();

    // Layer 2: File provides base configuration
    composer.push_file(
        json!({
            "engine_socket": "file-socket",
            "image": "file-image",
            "sandbox": { "privileged": true },
            "github": { "app_id": 100 }
        }),
        None,
    );

    // Layer 3: Environment overrides some values
    composer.push_environment(json!({
        "image": "env-image",
        "github": { "app_id": 200, "installation_id": 300 }
    }));

    // Layer 4: CLI overrides the highest priority values
    composer.push_cli(json!({
        "engine_socket": "cli-socket"
    }));

    let config = merge_config(composer);

    // CLI wins for engine_socket
    assert_eq!(config.engine_socket.as_deref(), Some("cli-socket"));
    // Env wins for image
    assert_eq!(config.image.as_deref(), Some("env-image"));
    // File wins for sandbox.privileged (not overridden by higher layers)
    assert!(config.sandbox.privileged);
    // Env wins for github.app_id (higher than file, no CLI override)
    assert_eq!(config.github.app_id, Some(200));
    // Env provides github.installation_id
    assert_eq!(config.github.installation_id, Some(300));
}

/// Test that nested config merges correctly across layers.
#[rstest]
fn layer_precedence_nested_config_merges() {
    let mut composer = create_composer_with_defaults();
    composer.push_file(
        json!({
            "sandbox": {
                "privileged": true,
                "mount_dev_fuse": false
            }
        }),
        None,
    );
    composer.push_environment(json!({
        "sandbox": {
            "privileged": false
        }
    }));

    let config = merge_config(composer);

    // Environment overrides file for privileged
    assert!(!config.sandbox.privileged);
    // File value preserved for mount_dev_fuse (not in env layer)
    assert!(!config.sandbox.mount_dev_fuse);
}

/// Test that missing layers result in defaults being used.
#[rstest]
fn layer_precedence_empty_layers_use_defaults() {
    let mut composer = create_composer_with_defaults();
    // Add empty override layers (no effect on values)
    composer.push_file(json!({}), None);
    composer.push_environment(json!({}));
    composer.push_cli(json!({}));

    let config = merge_config(composer);

    // All values should be defaults
    assert!(config.engine_socket.is_none());
    assert!(config.image.is_none());
    assert!(!config.sandbox.privileged);
    assert!(config.sandbox.mount_dev_fuse);
    assert_eq!(config.workspace.base_dir.as_str(), "/work");
}
