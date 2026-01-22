//! Basic type and serialisation tests for podbot configuration types.

use crate::config::{AgentConfig, AgentKind, AgentMode, AppConfig};
use rstest::rstest;

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
fn app_config_engine_and_image_default_to_none() {
    let config = AppConfig::default();
    assert!(config.engine_socket.is_none());
    assert!(config.image.is_none());
}

#[rstest]
fn app_config_sandbox_mount_dev_fuse_defaults_to_true() {
    let config = AppConfig::default();
    assert!(config.sandbox.mount_dev_fuse);
}

#[rstest]
fn app_config_sandbox_privileged_defaults_to_false() {
    let config = AppConfig::default();
    assert!(!config.sandbox.privileged);
}

#[rstest]
fn app_config_github_defaults_to_none() {
    let config = AppConfig::default();
    assert!(config.github.app_id.is_none());
    assert!(config.github.installation_id.is_none());
    assert!(config.github.private_key_path.is_none());
}

#[rstest]
fn app_config_agent_defaults_to_claude_podbot() {
    let config = AppConfig::default();
    assert_eq!(config.agent.kind, AgentKind::Claude);
    assert_eq!(config.agent.mode, AgentMode::Podbot);
}

#[rstest]
fn app_config_workspace_defaults_to_work_dir() {
    let config = AppConfig::default();
    assert_eq!(config.workspace.base_dir.as_str(), "/work");
}

#[rstest]
fn app_config_toml_sets_engine_socket_and_image() {
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
}

#[rstest]
fn app_config_toml_sets_github_ids() {
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
    assert_eq!(config.github.app_id, Some(12345));
    assert_eq!(config.github.installation_id, Some(67890));
}

#[rstest]
fn app_config_toml_sets_sandbox_flags() {
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
    assert!(config.sandbox.privileged);
    assert!(!config.sandbox.mount_dev_fuse);
}

#[rstest]
fn app_config_toml_sets_agent_config() {
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
    assert_eq!(config.agent.kind, AgentKind::Codex);
    assert_eq!(config.agent.mode, AgentMode::Podbot);
}

#[rstest]
fn app_config_toml_sets_workspace_base_dir() {
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
    assert_eq!(config.workspace.base_dir.as_str(), "/home/user/work");
}

#[rstest]
fn app_config_partial_toml_sets_engine_socket() {
    let toml = r#"
        engine_socket = "unix:///tmp/docker.sock"
    "#;

    let config: AppConfig = toml::from_str(toml).expect("TOML parsing should succeed");
    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///tmp/docker.sock")
    );
}

#[rstest]
fn app_config_partial_toml_image_defaults_to_none() {
    let toml = r#"
        engine_socket = "unix:///tmp/docker.sock"
    "#;

    let config: AppConfig = toml::from_str(toml).expect("TOML parsing should succeed");
    assert!(config.image.is_none());
}

#[rstest]
fn app_config_partial_toml_github_app_id_defaults_to_none() {
    let toml = r#"
        engine_socket = "unix:///tmp/docker.sock"
    "#;

    let config: AppConfig = toml::from_str(toml).expect("TOML parsing should succeed");
    assert!(config.github.app_id.is_none());
}

#[rstest]
fn app_config_partial_toml_sandbox_defaults_apply() {
    let toml = r#"
        engine_socket = "unix:///tmp/docker.sock"
    "#;

    let config: AppConfig = toml::from_str(toml).expect("TOML parsing should succeed");
    assert!(!config.sandbox.privileged);
    assert!(config.sandbox.mount_dev_fuse);
}

#[rstest]
fn app_config_partial_toml_agent_defaults_apply() {
    let toml = r#"
        engine_socket = "unix:///tmp/docker.sock"
    "#;

    let config: AppConfig = toml::from_str(toml).expect("TOML parsing should succeed");
    assert_eq!(config.agent.kind, AgentKind::Claude);
    assert_eq!(config.agent.mode, AgentMode::Podbot);
}

#[rstest]
fn app_config_partial_toml_workspace_default_applies() {
    let toml = r#"
        engine_socket = "unix:///tmp/docker.sock"
    "#;

    let config: AppConfig = toml::from_str(toml).expect("TOML parsing should succeed");
    assert_eq!(config.workspace.base_dir.as_str(), "/work");
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
        "Expected error mentioning to invalid value \"{value}\", got: {error}"
    );
}
