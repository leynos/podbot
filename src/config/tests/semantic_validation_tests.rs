//! Semantic validation tests for hosted-era configuration rules.

use camino::Utf8PathBuf;
use rstest::rstest;

use crate::config::{AgentKind, AgentMode, AppConfig, CommandIntent, WorkspaceSource};
use crate::error::{ConfigError, PodbotError};

#[rstest]
fn custom_agent_requires_command() {
    let mut config = AppConfig::default();
    config.agent.kind = AgentKind::Custom;
    config.agent.mode = AgentMode::CodexAppServer;

    assert_invalid_value(
        config.normalize_and_validate(CommandIntent::Host),
        "agent.command",
        "requires a non-empty",
    );
}

#[rstest]
fn builtin_agents_reject_custom_command_fields() {
    let mut config = AppConfig::default();
    config.agent.command = Some(String::from("opencode"));

    assert_invalid_value(
        config.normalize_and_validate(CommandIntent::Run),
        "agent.command",
        "built-in agent kinds",
    );
}

#[rstest]
fn host_mount_requires_host_path() {
    let mut config = AppConfig::default();
    config.workspace.source = WorkspaceSource::HostMount;
    config.agent.mode = AgentMode::Acp;

    assert_invalid_value(
        config.normalize_and_validate(CommandIntent::Host),
        "workspace.host_path",
        "requires `workspace.host_path`",
    );
}

#[rstest]
fn run_rejects_hosted_modes() {
    let mut config = AppConfig::default();
    config.agent.mode = AgentMode::CodexAppServer;

    assert_invalid_value(
        config.normalize_and_validate(CommandIntent::Run),
        "agent.mode",
        "hosted modes require `podbot host`",
    );
}

#[rstest]
fn host_rejects_podbot_mode() {
    let mut config = AppConfig::default();

    assert_invalid_value(
        config.normalize_and_validate(CommandIntent::Host),
        "agent.mode",
        "interactive mode requires `podbot run`",
    );
}

#[rstest]
fn github_clone_rejects_host_mount_fields() {
    let mut config = AppConfig::default();
    config.workspace.host_path = Some(Utf8PathBuf::from("/tmp/project"));

    assert_invalid_value(
        config.normalize_and_validate(CommandIntent::Any),
        "workspace.host_path",
        "only valid when `workspace.source = \"host_mount\"`",
    );
}

#[rstest]
fn workspace_base_dir_rejects_relative_path() {
    let mut config = AppConfig::default();
    config.workspace.base_dir = Utf8PathBuf::from("relative/path");

    assert_invalid_value(
        config.normalize_and_validate(CommandIntent::Any),
        "workspace.base_dir",
        "must be an absolute container path",
    );
}

#[rstest]
fn host_mount_rejects_relative_host_path() {
    let mut config = AppConfig::default();
    config.workspace.source = WorkspaceSource::HostMount;
    config.workspace.host_path = Some(Utf8PathBuf::from("relative/host"));

    assert_invalid_value(
        config.normalize_and_validate(CommandIntent::Any),
        "workspace.host_path",
        "must be an absolute host path",
    );
}

#[rstest]
fn host_mount_rejects_relative_container_path() {
    let mut config = AppConfig::default();
    config.workspace.source = WorkspaceSource::HostMount;
    config.workspace.host_path = Some(Utf8PathBuf::from("/tmp/project"));
    config.workspace.container_path = Some(Utf8PathBuf::from("relative/container"));

    assert_invalid_value(
        config.normalize_and_validate(CommandIntent::Any),
        "workspace.container_path",
        "must be an absolute container path",
    );
}

#[rstest]
fn env_allowlist_rejects_empty_or_whitespace_entries() {
    let mut config = AppConfig::default();
    config.agent.env_allowlist = vec![
        String::from("OPENAI_API_KEY"),
        String::new(),
        String::from("   "),
    ];

    assert_invalid_value(
        config.normalize_and_validate(CommandIntent::Any),
        "agent.env_allowlist",
        "must not be empty or whitespace only",
    );
}

fn assert_invalid_value(
    result: crate::error::Result<()>,
    expected_field: &str,
    expected_reason: &str,
) {
    let error = result.expect_err("validation should fail");

    match error {
        PodbotError::Config(ConfigError::InvalidValue { field, reason }) => {
            assert_eq!(field, expected_field);
            assert!(
                reason.contains(expected_reason),
                "expected '{reason}' to mention '{expected_reason}'"
            );
        }
        other => panic!("expected ConfigError::InvalidValue, got {other:?}"),
    }
}
