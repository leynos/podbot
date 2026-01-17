//! Unit tests for podbot configuration types.

use super::*;
use camino::Utf8PathBuf;
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

/// Fixture providing a fully configured `GitHubConfig`.
#[fixture]
fn github_config_complete() -> GitHubConfig {
    GitHubConfig {
        app_id: Some(12345),
        installation_id: Some(67890),
        private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
    }
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
fn app_config_has_sensible_defaults(app_config: AppConfig) {
    assert!(app_config.engine_socket.is_none());
    assert!(app_config.image.is_none());
    assert!(app_config.sandbox.mount_dev_fuse);
    assert!(!app_config.sandbox.privileged);
}

#[rstest]
fn app_config_nested_configs_have_defaults(app_config: AppConfig) {
    assert!(app_config.github.app_id.is_none());
    assert!(app_config.github.installation_id.is_none());
    assert!(app_config.github.private_key_path.is_none());
    assert_eq!(app_config.agent.kind, AgentKind::Claude);
    assert_eq!(app_config.agent.mode, AgentMode::Podbot);
    assert_eq!(app_config.workspace.base_dir.as_str(), "/work");
}

#[rstest]
fn app_config_deserialises_from_toml() {
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

    let config: AppConfig = toml::from_str(toml).expect("TOML parsing should succeed");

    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///run/podman/podman.sock")
    );
    assert_eq!(
        config.image.as_deref(),
        Some("ghcr.io/example/sandbox:latest")
    );
    assert_eq!(config.github.app_id, Some(12345));
    assert_eq!(config.github.installation_id, Some(67890));
    assert!(config.sandbox.privileged);
    assert!(!config.sandbox.mount_dev_fuse);
    assert_eq!(config.agent.kind, AgentKind::Codex);
    assert_eq!(config.agent.mode, AgentMode::Podbot);
    assert_eq!(config.workspace.base_dir.as_str(), "/home/user/work");
}

#[rstest]
fn app_config_uses_defaults_for_missing_fields() {
    let toml = r#"
        engine_socket = "unix:///tmp/docker.sock"
    "#;

    let config: AppConfig = toml::from_str(toml).expect("TOML parsing should succeed");

    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///tmp/docker.sock")
    );
    // All other fields should have defaults
    assert!(config.image.is_none());
    assert!(config.github.app_id.is_none());
    assert!(!config.sandbox.privileged);
    assert!(config.sandbox.mount_dev_fuse);
    assert_eq!(config.agent.kind, AgentKind::Claude);
    assert_eq!(config.agent.mode, AgentMode::Podbot);
    assert_eq!(config.workspace.base_dir.as_str(), "/work");
}

#[rstest]
fn app_config_rejects_invalid_agent_kind() {
    let toml = r#"
        [agent]
        kind = "unknown"
    "#;

    let error = toml::from_str::<AppConfig>(toml)
        .expect_err("TOML parsing should fail for an invalid agent kind");
    assert!(
        error.to_string().contains("unknown variant"),
        "Expected unknown-variant error, got: {error}"
    );
}

#[rstest]
fn app_config_rejects_invalid_agent_mode() {
    let toml = r#"
        [agent]
        mode = "unknown"
    "#;

    let error = toml::from_str::<AppConfig>(toml)
        .expect_err("TOML parsing should fail for an invalid agent mode");
    assert!(
        error.to_string().contains("unknown variant"),
        "Expected unknown-variant error, got: {error}"
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
