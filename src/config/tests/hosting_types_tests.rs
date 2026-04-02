//! Hosting-era schema defaults and serialization tests.

use std::collections::HashMap;

use camino::Utf8PathBuf;
use mockable::MockEnv;
use rstest::rstest;

use crate::config::{
    AgentKind, AgentMode, AppConfig, CommandIntent, ConfigLoadOptions, McpAllowedOriginPolicy,
    McpAuthTokenPolicy, McpBindStrategy, WorkspaceSource, load_config_with_env,
};

#[rstest]
fn app_config_hosting_defaults_are_explicit() {
    let config = AppConfig::default();

    assert_eq!(config.workspace.source, WorkspaceSource::GithubClone);
    assert!(config.workspace.host_path.is_none());
    assert!(config.workspace.container_path.is_none());
    assert!(config.agent.command.is_none());
    assert!(config.agent.args.is_empty());
    assert!(config.agent.env_allowlist.is_empty());
    assert_eq!(config.mcp.bind_strategy, McpBindStrategy::HostGateway);
    assert_eq!(config.mcp.idle_timeout_secs, 900);
    assert_eq!(config.mcp.max_message_size_bytes, 1_048_576);
    assert_eq!(
        config.mcp.auth_token_policy,
        McpAuthTokenPolicy::PerWorkspace
    );
    assert_eq!(
        config.mcp.allowed_origin_policy,
        McpAllowedOriginPolicy::SameOrigin
    );
}

#[rstest]
fn app_config_mcp_env_overrides() {
    let values = HashMap::from([
        (
            String::from("PODBOT_MCP_BIND_STRATEGY"),
            String::from("loopback"),
        ),
        (
            String::from("PODBOT_MCP_IDLE_TIMEOUT_SECS"),
            String::from("123"),
        ),
        (
            String::from("PODBOT_MCP_MAX_MESSAGE_SIZE_BYTES"),
            String::from("4096"),
        ),
        (
            String::from("PODBOT_MCP_AUTH_TOKEN_POLICY"),
            String::from("per_wire"),
        ),
        (
            String::from("PODBOT_MCP_ALLOWED_ORIGIN_POLICY"),
            String::from("any"),
        ),
    ]);
    let mut env = MockEnv::new();
    env.expect_string()
        .returning(move |key| values.get(key).cloned());

    let config = load_config_with_env(
        &env,
        &ConfigLoadOptions {
            discover_config: false,
            ..ConfigLoadOptions::default()
        },
    )
    .expect("config should load successfully from MCP env overrides");

    assert_eq!(config.mcp.bind_strategy, McpBindStrategy::Loopback);
    assert_eq!(config.mcp.idle_timeout_secs, 123);
    assert_eq!(config.mcp.max_message_size_bytes, 4096);
    assert_eq!(config.mcp.auth_token_policy, McpAuthTokenPolicy::PerWire);
    assert_eq!(
        config.mcp.allowed_origin_policy,
        McpAllowedOriginPolicy::Any
    );
}

#[rstest]
#[case(AgentKind::Custom, "custom")]
fn agent_kind_supports_hosting_variants(#[case] kind: AgentKind, #[case] expected: &str) {
    let serialised = serde_json::to_string(&kind).expect("serialisation should succeed");
    assert_eq!(serialised, format!("\"{expected}\""));
}

#[rstest]
#[case(AgentMode::CodexAppServer, "codex_app_server")]
#[case(AgentMode::Acp, "acp")]
fn agent_mode_supports_hosting_variants(#[case] mode: AgentMode, #[case] expected: &str) {
    let serialised = serde_json::to_string(&mode).expect("serialisation should succeed");
    assert_eq!(serialised, format!("\"{expected}\""));
}

#[rstest]
#[case(WorkspaceSource::GithubClone, "github_clone")]
#[case(WorkspaceSource::HostMount, "host_mount")]
fn workspace_source_serialises_to_snake_case(
    #[case] source: WorkspaceSource,
    #[case] expected: &str,
) {
    let serialised = serde_json::to_string(&source).expect("serialisation should succeed");
    assert_eq!(serialised, format!("\"{expected}\""));
}

#[rstest]
fn hosting_toml_deserialises() {
    let config = toml::from_str::<AppConfig>(
        r#"
        [workspace]
        source = "host_mount"
        host_path = "/tmp/project"

        [agent]
        kind = "custom"
        mode = "acp"
        command = "opencode"
        args = ["acp"]
        env_allowlist = ["OPENAI_API_KEY", "ANTHROPIC_API_KEY"]

        [mcp]
        bind_strategy = "loopback"
        idle_timeout_secs = 30
        max_message_size_bytes = 4096
        auth_token_policy = "per_wire"
        allowed_origin_policy = "any"
    "#,
    )
    .expect("TOML parsing should succeed");

    assert_eq!(config.workspace.source, WorkspaceSource::HostMount);
    assert_eq!(
        config.workspace.host_path,
        Some(Utf8PathBuf::from("/tmp/project"))
    );
    assert_eq!(config.agent.kind, AgentKind::Custom);
    assert_eq!(config.agent.mode, AgentMode::Acp);
    assert_eq!(config.agent.command.as_deref(), Some("opencode"));
    assert_eq!(config.agent.args, vec![String::from("acp")]);
    assert_eq!(
        config.agent.env_allowlist,
        vec![
            String::from("OPENAI_API_KEY"),
            String::from("ANTHROPIC_API_KEY"),
        ]
    );
    assert_eq!(config.mcp.bind_strategy, McpBindStrategy::Loopback);
    assert_eq!(config.mcp.idle_timeout_secs, 30);
    assert_eq!(config.mcp.max_message_size_bytes, 4096);
    assert_eq!(config.mcp.auth_token_policy, McpAuthTokenPolicy::PerWire);
    assert_eq!(
        config.mcp.allowed_origin_policy,
        McpAllowedOriginPolicy::Any
    );
}

#[rstest]
fn normalize_and_validate_adds_host_mount_container_default() {
    let mut config = AppConfig::default();
    config.workspace.source = WorkspaceSource::HostMount;
    config.workspace.host_path = Some(Utf8PathBuf::from("/tmp/project"));
    config.agent.mode = AgentMode::CodexAppServer;

    config
        .normalize_and_validate(CommandIntent::Host)
        .expect("host config should validate");

    assert_eq!(
        config.workspace.container_path,
        Some(Utf8PathBuf::from("/workspace"))
    );
}
