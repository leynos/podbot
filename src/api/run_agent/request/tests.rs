//! Unit tests for run-plan construction and agent command mapping.

use camino::{Utf8Path, Utf8PathBuf};
use chrono::{DateTime, Utc};
use mockable::{MockClock, MockEnv};
use rstest::rstest;
use tempfile::TempDir;

use super::{
    DEFAULT_ASKPASS_PATH, agent_command, agent_exec_request, allowed_environment, build_run_plan,
    container_name, host_home_dir, needs_github_token, session_id,
};
use crate::api::RunRequest;
use crate::config::{AgentKind, AgentMode, AppConfig, WorkspaceSource};
use crate::engine::MANAGED_LABEL_KEY;
use crate::error::{ConfigError, PodbotError};

fn fixed_clock() -> MockClock {
    let mut clock = MockClock::new();
    clock
        .expect_utc()
        .returning(|| DateTime::<Utc>::from_timestamp(1_700_000_000, 0).expect("valid timestamp"));
    clock
}

fn runtime_dir(temp: &TempDir) -> String {
    String::from(
        Utf8Path::from_path(temp.path())
            .expect("temp path should be UTF-8")
            .as_str(),
    )
}

fn clone_config(image: &str) -> AppConfig {
    AppConfig {
        image: Some(String::from(image)),
        workspace: crate::config::WorkspaceConfig {
            source: WorkspaceSource::GithubClone,
            ..crate::config::WorkspaceConfig::default()
        },
        ..AppConfig::default()
    }
}

fn host_mount_config(host_path: &Utf8Path) -> AppConfig {
    AppConfig {
        image: Some(String::from("podbot-sandbox:latest")),
        workspace: crate::config::WorkspaceConfig {
            source: WorkspaceSource::HostMount,
            host_path: Some(host_path.to_owned()),
            container_path: Some(Utf8PathBuf::from("/workspace")),
            ..crate::config::WorkspaceConfig::default()
        },
        ..AppConfig::default()
    }
}

fn env_with(values: Vec<(&'static str, String)>) -> MockEnv {
    let mut env = MockEnv::new();
    env.expect_string().returning(move |key| {
        values
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.clone())
    });
    env
}

#[test]
fn session_id_combines_process_id_and_clock_nanos() {
    let clock = fixed_clock();

    let first = session_id(&clock);
    let second = session_id(&clock);

    assert_eq!(first, second);
    assert!(first.starts_with(&std::process::id().to_string()));
    assert!(first.contains("1700000000000000000"));
}

#[test]
fn container_name_prefixes_the_session_id() {
    assert_eq!(container_name("session-1"), "podbot-session-1");
}

#[rstest]
#[case::claude(AgentKind::Claude, vec!["claude", "--dangerously-skip-permissions"])]
#[case::codex(
    AgentKind::Codex,
    vec!["codex", "--dangerously-bypass-approvals-and-sandbox"]
)]
fn default_agent_command_maps_known_kinds(#[case] kind: AgentKind, #[case] expected: Vec<&str>) {
    let mut config = AppConfig::default();
    config.agent.kind = kind;

    let command = agent_command(&config).expect("agent command should be built");

    assert_eq!(
        command.iter().map(String::as_str).collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn custom_agent_command_uses_configured_executable_and_args() {
    let mut config = AppConfig::default();
    config.agent.kind = AgentKind::Custom;
    config.agent.command = Some(String::from("my-agent"));
    config.agent.args = vec![String::from("--flag"), String::from("value")];

    let command = agent_command(&config).expect("custom agent command should be built");

    assert_eq!(
        command.iter().map(String::as_str).collect::<Vec<_>>(),
        ["my-agent", "--flag", "value"]
    );
}

#[test]
fn custom_agent_without_executable_is_rejected() {
    let mut config = AppConfig::default();
    config.agent.kind = AgentKind::Custom;

    let error = agent_command(&config).expect_err("missing executable should be rejected");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field })
            if field == "agent.command"
    ));
}

#[test]
fn non_interactive_agent_mode_is_rejected() {
    let mut config = AppConfig::default();
    config.agent.mode = AgentMode::CodexAppServer;

    let error = agent_command(&config).expect_err("protocol mode should be rejected for run");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::InvalidValue { field, .. }) if field == "agent.mode"
    ));
}

#[test]
fn clone_plan_mounts_the_token_store_read_only() {
    let temp = tempfile::tempdir().expect("temp dir should be created");
    let env = env_with(vec![("XDG_RUNTIME_DIR", runtime_dir(&temp))]);
    let config = clone_config("podbot-sandbox:latest");
    let request = RunRequest::new("owner/name", "main").expect("request should be valid");

    let plan = build_run_plan(&config, &request, &fixed_clock(), &env)
        .expect("clone plan should be built");

    assert_eq!(plan.workspace_path.as_str(), "/work/name");
    assert_eq!(plan.create_request.working_dir(), Some("/work/name"));
    assert_eq!(
        plan.create_request.name(),
        Some(plan.container_name.as_str())
    );
    assert!(
        plan.create_request
            .labels()
            .iter()
            .any(|(key, value)| key == MANAGED_LABEL_KEY && value == "true")
    );

    let mounts = plan.create_request.mounts();
    assert_eq!(mounts.len(), 1);
    let token_mount = mounts.first().expect("token mount should be present");
    assert_eq!(token_mount.target().as_str(), "/run/secrets");
    assert!(token_mount.is_read_only());
    assert!(plan.token_store.is_some());
}

#[test]
fn host_mount_plan_mounts_the_workspace_read_write() {
    let temp = tempfile::tempdir().expect("temp dir should be created");
    let host_path = Utf8Path::from_path(temp.path()).expect("temp path should be UTF-8");
    let env = env_with(vec![("XDG_RUNTIME_DIR", runtime_dir(&temp))]);
    let config = host_mount_config(host_path);
    let request = RunRequest::new("owner/name", "main").expect("request should be valid");

    let plan = build_run_plan(&config, &request, &fixed_clock(), &env)
        .expect("host-mount plan should be built");

    assert_eq!(plan.workspace_path.as_str(), "/workspace");
    assert!(plan.token_store.is_none());
    let mounts = plan.create_request.mounts();
    assert_eq!(mounts.len(), 1);
    let workspace_mount = mounts.first().expect("workspace mount should be present");
    assert!(!workspace_mount.is_read_only());
    assert_eq!(workspace_mount.source().as_str(), host_path.as_str());
}

#[test]
fn host_mount_requires_a_host_path() {
    let temp = tempfile::tempdir().expect("temp dir should be created");
    let env = env_with(vec![("XDG_RUNTIME_DIR", runtime_dir(&temp))]);
    let mut config = host_mount_config(Utf8Path::new("/tmp/unused"));
    config.workspace.host_path = None;
    let request = RunRequest::new("owner/name", "main").expect("request should be valid");

    let error = build_run_plan(&config, &request, &fixed_clock(), &env)
        .err()
        .expect("missing host path should be rejected");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field })
            if field == "workspace.host_path"
    ));
}

#[test]
fn allowlist_copies_present_values_and_skips_missing_ones() {
    let mut config = AppConfig::default();
    config.agent.env_allowlist = vec![
        String::from("ANTHROPIC_API_KEY"),
        String::from("MISSING_KEY"),
    ];
    let env = env_with(vec![("ANTHROPIC_API_KEY", String::from("secret-value"))]);

    let entries = allowed_environment(&config, &env);

    assert_eq!(
        entries.iter().map(String::as_str).collect::<Vec<_>>(),
        ["ANTHROPIC_API_KEY=secret-value"]
    );
}

#[test]
fn host_home_dir_requires_a_non_blank_value() {
    let env = env_with(vec![("HOME", String::new())]);

    let error = host_home_dir(&env).expect_err("blank HOME should be rejected");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field }) if field == "HOME"
    ));
}

#[test]
fn github_token_is_only_needed_for_clone_workspaces() {
    let temp = tempfile::tempdir().expect("temp dir should be created");
    let host_path = Utf8Path::from_path(temp.path()).expect("temp path should be UTF-8");

    assert!(needs_github_token(&clone_config("image")));
    assert!(!needs_github_token(&host_mount_config(host_path)));
}

#[test]
fn agent_exec_request_attaches_a_tty() {
    let config = AppConfig::default();

    let request = agent_exec_request(&config, "container-1").expect("exec request should be built");

    assert_eq!(request.container(), "container-1");
    assert!(request.tty());
    assert_eq!(
        request.command().first().map(String::as_str),
        Some("claude")
    );
    assert_eq!(DEFAULT_ASKPASS_PATH, "/usr/local/bin/git-askpass");
}
