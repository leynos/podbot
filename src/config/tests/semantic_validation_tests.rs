//! Semantic validation tests for hosted-era configuration rules.

use camino::Utf8PathBuf;
use rstest::rstest;

use crate::config::{AgentKind, AgentMode, AppConfig, CommandIntent, WorkspaceSource};
use crate::error::{ConfigError, PodbotError};

struct HostMountCase {
    host_path: Option<Utf8PathBuf>,
    container_path: Option<Utf8PathBuf>,
    agent_mode: Option<AgentMode>,
    intent: CommandIntent,
    expected_field: &'static str,
    expected_reason: &'static str,
}

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
#[case(HostMountCase {
    host_path: None,
    container_path: None,
    agent_mode: Some(AgentMode::Acp),
    intent: CommandIntent::Host,
    expected_field: "workspace.host_path",
    expected_reason: "requires `workspace.host_path`",
})]
#[case(HostMountCase {
    host_path: Some(Utf8PathBuf::from("relative/host")),
    container_path: None,
    agent_mode: None,
    intent: CommandIntent::Any,
    expected_field: "workspace.host_path",
    expected_reason: "must be an absolute host path",
})]
#[case(HostMountCase {
    host_path: Some(Utf8PathBuf::from("/tmp/project")),
    container_path: Some(Utf8PathBuf::from("relative/container")),
    agent_mode: None,
    intent: CommandIntent::Any,
    expected_field: "workspace.container_path",
    expected_reason: "must be an absolute container path",
})]
fn host_mount_validation(#[case] case: HostMountCase) {
    let mut config = AppConfig::default();
    config.workspace.source = WorkspaceSource::HostMount;

    if let Some(path) = case.host_path {
        config.workspace.host_path = Some(path);
    }

    if let Some(path) = case.container_path {
        config.workspace.container_path = Some(path);
    }

    if let Some(agent_mode) = case.agent_mode {
        config.agent.mode = agent_mode;
    }

    assert_invalid_value(
        config.normalize_and_validate(case.intent),
        case.expected_field,
        case.expected_reason,
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
