//! Integration tests for hosted-era configuration loading.

mod test_support;

use std::io::Write;

use crate::test_support::env_with;
use camino::Utf8PathBuf;
use podbot::config::{
    AgentKind, AgentMode, CommandIntent, ConfigLoadOptions, ConfigOverrides, WorkspaceSource,
    load_config_with_env,
};
use podbot::error::{ConfigError, PodbotError};
use rstest::rstest;
use tempfile::NamedTempFile;

fn temp_config_file(content: &str) -> std::io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    file.write_all(content.as_bytes())?;
    Ok(file)
}

#[rstest]
fn load_config_normalizes_host_mount_defaults() {
    let config_file = temp_config_file(
        r#"
        [workspace]
        source = "host_mount"
        host_path = "/tmp/project"

        [agent]
        kind = "custom"
        mode = "codex_app_server"
        command = "opencode"
    "#,
    )
    .expect("temp config file creation should succeed");
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");
    let env = env_with(&[]);

    let config = load_config_with_env(
        &env,
        &ConfigLoadOptions {
            config_path_hint: Some(config_path),
            discover_config: false,
            command_intent: CommandIntent::Host,
            ..ConfigLoadOptions::default()
        },
    )
    .expect("host config should load");

    assert_eq!(config.workspace.source, WorkspaceSource::HostMount);
    assert_eq!(config.workspace.host_path, Some("/tmp/project".into()));
    assert_eq!(config.workspace.container_path, Some("/workspace".into()));
    assert_eq!(config.agent.kind, AgentKind::Custom);
    assert_eq!(config.agent.mode, AgentMode::CodexAppServer);
}

#[rstest]
fn load_config_applies_env_overrides_for_hosting_fields() {
    let env = env_with(&[
        ("PODBOT_WORKSPACE_SOURCE", "host_mount"),
        ("PODBOT_WORKSPACE_HOST_PATH", "/tmp/project"),
        ("PODBOT_AGENT_KIND", "custom"),
        ("PODBOT_AGENT_MODE", "acp"),
        ("PODBOT_AGENT_COMMAND", "opencode"),
        ("PODBOT_AGENT_ARGS", " serve , --json "),
        (
            "PODBOT_AGENT_ENV_ALLOWLIST",
            ",OPENAI_API_KEY,,ANTHROPIC_API_KEY,",
        ),
    ]);

    let config = load_config_with_env(
        &env,
        &ConfigLoadOptions {
            discover_config: false,
            command_intent: CommandIntent::Host,
            ..ConfigLoadOptions::default()
        },
    )
    .expect("hosting env vars should load");

    assert_eq!(config.workspace.source, WorkspaceSource::HostMount);
    assert_eq!(config.workspace.host_path, Some("/tmp/project".into()));
    assert_eq!(config.workspace.container_path, Some("/workspace".into()));
    assert_eq!(config.agent.kind, AgentKind::Custom);
    assert_eq!(config.agent.mode, AgentMode::Acp);
    assert_eq!(config.agent.command.as_deref(), Some("opencode"));
    assert_eq!(
        config.agent.args,
        vec![String::from("serve"), String::from("--json")]
    );
    assert_eq!(
        config.agent.env_allowlist,
        vec![
            String::from("OPENAI_API_KEY"),
            String::from("ANTHROPIC_API_KEY"),
        ]
    );
}

#[rstest]
fn load_config_rejects_hosted_mode_for_run_intent() {
    let env = env_with(&[
        ("PODBOT_AGENT_KIND", "custom"),
        ("PODBOT_AGENT_MODE", "acp"),
        ("PODBOT_AGENT_COMMAND", "opencode"),
    ]);

    let error = load_config_with_env(
        &env,
        &ConfigLoadOptions {
            discover_config: false,
            command_intent: CommandIntent::Run,
            ..ConfigLoadOptions::default()
        },
    )
    .expect_err("run intent should reject hosted modes");

    assert_invalid_value(error, "agent.mode", "hosted modes require `podbot host`");
}

#[rstest]
fn load_config_rejects_custom_agent_without_command() {
    let options = ConfigLoadOptions {
        discover_config: false,
        command_intent: CommandIntent::Host,
        overrides: ConfigOverrides {
            engine_socket: None,
            image: None,
            agent_kind: Some(AgentKind::Custom),
            agent_mode: Some(AgentMode::CodexAppServer),
        },
        ..ConfigLoadOptions::default()
    };

    let error = load_config_with_env(&env_with(&[]), &options)
        .expect_err("custom hosted agent should require a command");

    assert_invalid_value(error, "agent.command", "requires a non-empty");
}

#[rstest]
fn hosted_agent_kind_and_mode_follow_override_env_file_precedence() {
    let config_file = temp_config_file(
        r#"
        [agent]
        kind = "claude"
        mode = "podbot"
    "#,
    )
    .expect("temp config file creation should succeed");
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");
    let env = env_with(&[
        ("PODBOT_AGENT_KIND", "codex"),
        ("PODBOT_AGENT_MODE", "acp"),
        ("PODBOT_AGENT_COMMAND", "opencode"),
    ]);
    let options = ConfigLoadOptions {
        config_path_hint: Some(config_path),
        discover_config: false,
        command_intent: CommandIntent::Host,
        overrides: ConfigOverrides {
            engine_socket: None,
            image: None,
            agent_kind: Some(AgentKind::Custom),
            agent_mode: Some(AgentMode::CodexAppServer),
        },
    };

    let config = load_config_with_env(&env, &options)
        .expect("override values should take precedence in hosted config");

    assert_eq!(config.agent.kind, AgentKind::Custom);
    assert_eq!(config.agent.mode, AgentMode::CodexAppServer);
    assert_eq!(config.agent.command.as_deref(), Some("opencode"));
}

fn assert_invalid_value(error: PodbotError, expected_field: &str, expected_reason: &str) {
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
