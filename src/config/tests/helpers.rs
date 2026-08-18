//! Shared fixtures and helper functions for config tests.

use crate::config::{AgentKind, AgentMode, AppConfig, GitHubConfig, SelinuxLabelMode};
use camino::Utf8PathBuf;
use ortho_config::MergeComposer;
use rstest::fixture;
use std::sync::Arc;

/// Convenience alias for the fallible `serde_json` result produced when
/// building a `MergeComposer` fixture.
type ComposerResult = Result<MergeComposer, serde_json::Error>;

/// Fixture providing an `AppConfig` parsed from a full TOML example.
///
/// Returns a `Result` so parsing failures are reported by the consuming
/// test with test-specific context via `.expect(...)`, rather than
/// panicking inside the fixture itself.
#[fixture]
pub fn app_config_from_full_toml() -> Result<AppConfig, toml::de::Error> {
    let toml = r#"
        engine_socket = "unix:///run/podman/podman.sock"
        image = "ghcr.io/example/sandbox:latest"

        [github]
        app_id = 12345
        installation_id = 67890

        [sandbox]
        privileged = true
        mount_dev_fuse = false
        selinux_label_mode = "keep_default"

        [agent]
        kind = "codex"
        mode = "podbot"

        [workspace]
        base_dir = "/home/user/work"
    "#;

    toml::from_str(toml)
}

/// Fixture providing an `AppConfig` parsed from a minimal TOML example.
///
/// Returns a `Result`; see [`app_config_from_full_toml`] for rationale.
#[fixture]
pub fn app_config_from_partial_toml() -> Result<AppConfig, toml::de::Error> {
    let toml = r#"
        engine_socket = "unix:///tmp/docker.sock"
    "#;

    toml::from_str(toml)
}

/// Fixture providing a fully configured `GitHubConfig`.
#[fixture]
pub fn github_config_complete() -> GitHubConfig {
    GitHubConfig {
        app_id: Some(12345),
        installation_id: Some(67890),
        private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
    }
}

/// Fixture providing a `MergeComposer` with the defaults layer already
/// pushed.
///
/// Returns a `Result` so callers report construction failures with
/// test-specific context via `.expect(...)`.
#[fixture]
pub fn create_composer_with_defaults() -> ComposerResult {
    let mut composer = MergeComposer::new();
    let defaults = ortho_config::serde_json::to_value(AppConfig::default())?;
    composer.push_defaults(defaults);
    Ok(composer)
}

/// Helper: Merges layers from a composer into `AppConfig`.
pub fn merge_config(composer: MergeComposer) -> Result<AppConfig, Arc<ortho_config::OrthoError>> {
    AppConfig::merge_from_layers(composer.layers())
}

/// Helper: Asserts that a config's agent section has default values.
pub fn assert_agent_defaults(config: &AppConfig) {
    assert_eq!(
        config.agent.kind,
        AgentKind::Claude,
        "agent.kind should be Claude"
    );
    assert_eq!(
        config.agent.mode,
        AgentMode::Podbot,
        "agent.mode should be Podbot"
    );
}

/// Helper: Asserts that a config's creds section has default values.
pub fn assert_creds_defaults(config: &AppConfig) {
    assert!(config.creds.copy_claude, "creds.copy_claude should be true");
    assert!(config.creds.copy_codex, "creds.copy_codex should be true");
}

/// Helper: Asserts that a config has all default values.
pub fn assert_config_has_defaults(config: &AppConfig) {
    assert!(
        config.engine_socket.is_none(),
        "engine_socket should be None"
    );
    assert!(config.image.is_none(), "image should be None");
    assert!(
        !config.sandbox.privileged,
        "sandbox.privileged should be false"
    );
    assert!(
        config.sandbox.mount_dev_fuse,
        "sandbox.mount_dev_fuse should be true"
    );
    assert_eq!(
        config.sandbox.selinux_label_mode,
        SelinuxLabelMode::DisableForContainer,
        "sandbox.selinux_label_mode should be DisableForContainer"
    );
    assert_eq!(
        config.workspace.base_dir.as_str(),
        "/work",
        "workspace.base_dir should be /work"
    );
    assert_agent_defaults(config);
    assert_creds_defaults(config);
}

/// Fixture providing a `MergeComposer` with defaults, file, and env layers
/// for testing layer precedence.
///
/// This builder pattern reduces duplication in tests that verify environment
/// and CLI layer precedence by providing pre-configured file and environment
/// layers. Returns a `Result`; see [`create_composer_with_defaults`] for rationale.
#[fixture]
pub fn create_composer_with_file_and_env() -> ComposerResult {
    use ortho_config::serde_json::json;

    let mut composer = MergeComposer::new();
    let defaults = ortho_config::serde_json::to_value(AppConfig::default())?;
    composer.push_defaults(defaults);

    // Standard file layer for precedence tests
    composer.push_file(
        json!({
            "engine_socket": "unix:///from/file.sock",
            "image": "file-image:latest"
        }),
        None,
    );

    // Standard environment layer for precedence tests
    composer.push_environment(json!({
        "engine_socket": "unix:///from/env.sock"
    }));

    Ok(composer)
}
