//! Behavioural step definitions for hosted-era configuration semantics.
#![allow(
    unfulfilled_lint_expectations,
    reason = "the task requires preserving step-level expectations after extracting the shared setup helper"
)]

use camino::Utf8PathBuf;
use podbot::config::{AgentKind, AgentMode, AppConfig, CommandIntent, WorkspaceSource};
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, then, when};

type StepResult<T> = Result<T, String>;

#[derive(Default, ScenarioState)]
/// State shared across hosted configuration scenarios.
pub struct HostingConfigState {
    config: Slot<AppConfig>,
    error: Slot<String>,
}

#[fixture]
/// Fixture providing a fresh hosted configuration state.
pub fn hosting_config_state() -> HostingConfigState {
    HostingConfigState::default()
}

#[given("the default application configuration")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions return StepResult for consistency"
)]
fn the_default_application_configuration(
    hosting_config_state: &HostingConfigState,
) -> StepResult<()> {
    hosting_config_state.config.set(AppConfig::default());
    Ok(())
}

#[derive(Default)]
struct HostingStateOptions {
    workspace_source: Option<WorkspaceSource>,
    workspace_host_path: Option<Utf8PathBuf>,
    agent_kind: Option<AgentKind>,
    agent_mode: Option<AgentMode>,
    agent_command: Option<String>,
}

fn configure_hosting_state(
    hosting_config_state: &HostingConfigState,
    opts: HostingStateOptions,
) -> StepResult<()> {
    if hosting_config_state.config.get().is_some() {
        return Err(String::from("config should not already be set"));
    }

    let mut config = AppConfig::default();

    if let Some(source) = opts.workspace_source {
        config.workspace.source = source;
    }
    if let Some(host_path) = opts.workspace_host_path {
        config.workspace.host_path = Some(host_path);
    }
    if let Some(kind) = opts.agent_kind {
        config.agent.kind = kind;
    }
    if let Some(mode) = opts.agent_mode {
        config.agent.mode = mode;
    }
    config.agent.command = opts.agent_command;

    hosting_config_state.config.set(config);
    Ok(())
}

macro_rules! given_hosting_step {
    (
        $fn_name:ident,
        $given_str:literal,
        {
            workspace_source: $workspace_source:expr,
            workspace_host_path: $workspace_host_path:expr,
            agent_kind: $agent_kind:expr,
            agent_mode: $agent_mode:expr,
            agent_command: $agent_command:expr $(,)?
        }
    ) => {
        #[given($given_str)]
        #[expect(
            clippy::unnecessary_wraps,
            reason = "rstest-bdd step functions return StepResult for consistency"
        )]
        fn $fn_name(hosting_config_state: &HostingConfigState) -> StepResult<()> {
            configure_hosting_state(
                hosting_config_state,
                HostingStateOptions {
                    workspace_source: $workspace_source,
                    workspace_host_path: $workspace_host_path,
                    agent_kind: $agent_kind,
                    agent_mode: $agent_mode,
                    agent_command: $agent_command,
                },
            )
        }
    };
    (
        $fn_name:ident,
        $given_str:literal,
        { $($field:ident: $value:expr),* $(,)? }
    ) => {
        #[given($given_str)]
        #[expect(
            clippy::unnecessary_wraps,
            reason = "rstest-bdd step functions return StepResult for consistency"
        )]
        fn $fn_name(hosting_config_state: &HostingConfigState) -> StepResult<()> {
            configure_hosting_state(
                hosting_config_state,
                HostingStateOptions {
                    $($field: $value,)*
                    ..Default::default()
                },
            )
        }
    };
}

given_hosting_step!(
    a_host_mounted_custom_agent_configuration,
    "a host-mounted custom agent configuration",
    {
        workspace_source: Some(WorkspaceSource::HostMount),
        workspace_host_path: Some(Utf8PathBuf::from("/tmp/project")),
        agent_kind: Some(AgentKind::Custom),
        agent_mode: Some(AgentMode::CodexAppServer),
        agent_command: Some(String::from("opencode")),
    }
);

given_hosting_step!(
    a_hosted_custom_agent_configuration,
    "a hosted custom agent configuration",
    {
        agent_kind: Some(AgentKind::Custom),
        agent_mode: Some(AgentMode::Acp),
        agent_command: Some(String::from("opencode")),
    }
);

given_hosting_step!(
    a_host_mounted_workspace_without_a_host_path,
    "a host-mounted workspace without a host path",
    {
        workspace_source: Some(WorkspaceSource::HostMount),
        agent_kind: Some(AgentKind::Custom),
        agent_mode: Some(AgentMode::CodexAppServer),
        agent_command: Some(String::from("opencode")),
    }
);

given_hosting_step!(
    a_custom_hosted_agent_without_a_command,
    "a custom hosted agent without a command",
    {
        agent_kind: Some(AgentKind::Custom),
        agent_mode: Some(AgentMode::CodexAppServer),
    }
);

#[when("the configuration is normalized for run intent")]
fn the_configuration_is_normalized_for_run_intent(
    hosting_config_state: &HostingConfigState,
) -> StepResult<()> {
    normalize_for_intent(hosting_config_state, CommandIntent::Run)
}

#[when("the configuration is normalized for host intent")]
fn the_configuration_is_normalized_for_host_intent(
    hosting_config_state: &HostingConfigState,
) -> StepResult<()> {
    normalize_for_intent(hosting_config_state, CommandIntent::Host)
}

#[then("the normalized configuration uses github_clone workspace defaults")]
fn the_normalized_configuration_uses_github_clone_workspace_defaults(
    hosting_config_state: &HostingConfigState,
) -> StepResult<()> {
    let config = get_config(hosting_config_state)?;
    assert_eq!(config.workspace.source, WorkspaceSource::GithubClone);
    assert_eq!(config.workspace.base_dir.as_str(), "/work");
    Ok(())
}

#[then("the normalized configuration uses podbot agent defaults")]
fn the_normalized_configuration_uses_podbot_agent_defaults(
    hosting_config_state: &HostingConfigState,
) -> StepResult<()> {
    let config = get_config(hosting_config_state)?;
    assert_eq!(config.agent.kind, AgentKind::Claude);
    assert_eq!(config.agent.mode, AgentMode::Podbot);
    Ok(())
}

#[then("the workspace container path defaults to /workspace")]
fn the_workspace_container_path_defaults_to_workspace(
    hosting_config_state: &HostingConfigState,
) -> StepResult<()> {
    let config = get_config(hosting_config_state)?;
    assert_eq!(config.workspace.container_path, Some("/workspace".into()));
    Ok(())
}

#[then("semantic validation fails for agent.mode mentioning podbot host")]
fn semantic_validation_fails_for_agent_mode_mentioning_podbot_host(
    hosting_config_state: &HostingConfigState,
) -> StepResult<()> {
    assert_invalid_value(hosting_config_state, "agent.mode", "podbot host")
}

#[then("semantic validation fails for workspace.host_path mentioning host_mount")]
fn semantic_validation_fails_for_workspace_host_path_mentioning_host_mount(
    hosting_config_state: &HostingConfigState,
) -> StepResult<()> {
    assert_invalid_value(hosting_config_state, "workspace.host_path", "host_mount")
}

#[then("semantic validation fails for agent.command mentioning requires a non-empty")]
fn semantic_validation_fails_for_agent_command_mentioning_requires_a_non_empty(
    hosting_config_state: &HostingConfigState,
) -> StepResult<()> {
    assert_invalid_value(
        hosting_config_state,
        "agent.command",
        "requires a non-empty",
    )
}

fn normalize_for_intent(
    hosting_config_state: &HostingConfigState,
    intent: CommandIntent,
) -> StepResult<()> {
    let mut config = get_config(hosting_config_state)?;

    match config.normalize_and_validate(intent) {
        Ok(()) => hosting_config_state.config.set(config),
        Err(error) => hosting_config_state.error.set(error.to_string()),
    }

    Ok(())
}

fn get_config(hosting_config_state: &HostingConfigState) -> StepResult<AppConfig> {
    hosting_config_state
        .config
        .get()
        .ok_or_else(|| String::from("config should be set"))
}

fn assert_invalid_value(
    hosting_config_state: &HostingConfigState,
    expected_field: &str,
    expected_reason: &str,
) -> StepResult<()> {
    let error = hosting_config_state
        .error
        .get()
        .ok_or_else(|| String::from("semantic error should be set"))?;

    if !error.contains(expected_field) {
        return Err(format!(
            "expected '{error}' to mention field '{expected_field}'"
        ));
    }
    if !error.contains(expected_reason) {
        return Err(format!("expected '{error}' to mention '{expected_reason}'"));
    }
    Ok(())
}
