//! Basic type and serialisation tests for podbot configuration types.

use crate::config::tests::helpers::{app_config_from_full_toml, app_config_from_partial_toml};
use crate::config::{AgentConfig, AgentKind, AgentMode, AppConfig, SelinuxLabelMode};
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
fn selinux_label_mode_default_is_disable_for_container() {
    assert_eq!(
        SelinuxLabelMode::default(),
        SelinuxLabelMode::DisableForContainer
    );
}

#[rstest]
#[case(SelinuxLabelMode::KeepDefault, "keep_default")]
#[case(SelinuxLabelMode::DisableForContainer, "disable_for_container")]
fn selinux_label_mode_serialises_to_snake_case(
    #[case] mode: SelinuxLabelMode,
    #[case] expected: &str,
) {
    let serialised = serde_json::to_string(&mode).expect("serialisation should succeed");
    assert_eq!(serialised, format!("\"{expected}\""));
}

#[rstest]
#[case("\"keep_default\"", SelinuxLabelMode::KeepDefault)]
#[case("\"disable_for_container\"", SelinuxLabelMode::DisableForContainer)]
fn selinux_label_mode_deserialises_from_snake_case(
    #[case] input: &str,
    #[case] expected: SelinuxLabelMode,
) {
    let mode: SelinuxLabelMode =
        serde_json::from_str(input).expect("deserialisation should succeed");
    assert_eq!(mode, expected);
}

#[rstest]
fn app_config_sandbox_selinux_label_mode_defaults_to_disable() {
    let config = AppConfig::default();
    assert_eq!(
        config.sandbox.selinux_label_mode,
        SelinuxLabelMode::DisableForContainer
    );
}

#[rstest]
fn app_config_toml_sets_selinux_label_mode(app_config_from_full_toml: AppConfig) {
    assert_eq!(
        app_config_from_full_toml.sandbox.selinux_label_mode,
        SelinuxLabelMode::KeepDefault
    );
}

#[rstest]
fn app_config_partial_toml_selinux_label_mode_defaults(app_config_from_partial_toml: AppConfig) {
    assert_eq!(
        app_config_from_partial_toml.sandbox.selinux_label_mode,
        SelinuxLabelMode::DisableForContainer
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

#[rstest]
fn app_config_rejects_invalid_selinux_label_mode() {
    let toml = r#"
        [sandbox]
        selinux_label_mode = "banana"
    "#;

    let error = toml::from_str::<AppConfig>(toml)
        .expect_err("TOML parsing should fail for an invalid selinux_label_mode");
    assert!(
        error.to_string().contains("banana"),
        "Expected error mentioning the invalid value, got: {error}"
    );
}
