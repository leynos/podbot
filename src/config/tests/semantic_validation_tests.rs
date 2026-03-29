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
