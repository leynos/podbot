//! Behavioural step definitions for hosted library configuration loading.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use camino::Utf8PathBuf;
use mockable::MockEnv;
use podbot::config::{CommandIntent, ConfigLoadOptions, WorkspaceSource, load_config_with_env};
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, then, when};

type StepResult<T> = Result<T, String>;
type EnvVars = Arc<Mutex<HashMap<String, String>>>;

#[derive(Default, ScenarioState)]
/// State shared across hosted library configuration scenarios.
pub struct HostingConfigLoaderState {
    env_vars: Slot<EnvVars>,
    temp_dir: Slot<Arc<tempfile::TempDir>>,
    options: Slot<ConfigLoadOptions>,
    config: Slot<podbot::config::AppConfig>,
    error: Slot<String>,
}

#[fixture]
/// Fixture providing a fresh hosting loader state.
pub fn hosting_config_loader_state() -> HostingConfigLoaderState {
    let state = HostingConfigLoaderState::default();
    state.env_vars.set(Arc::new(Mutex::new(HashMap::new())));
    state.options.set(ConfigLoadOptions {
        discover_config: false,
        command_intent: CommandIntent::Host,
        ..ConfigLoadOptions::default()
    });
    state
}

#[given("a hosting configuration file for a custom codex app server")]
fn a_hosting_configuration_file_for_a_custom_codex_app_server(
    hosting_config_loader_state: &HostingConfigLoaderState,
) -> StepResult<()> {
    let config_path = write_config_file(
        hosting_config_loader_state,
        r#"
        [workspace]
        source = "host_mount"
        host_path = "/tmp/project"

        [agent]
        kind = "custom"
        mode = "codex_app_server"
        command = "opencode"
    "#,
    )?;
    let mut options = get_options(hosting_config_loader_state)?;
    options.config_path_hint = Some(config_path);
    hosting_config_loader_state.options.set(options);
    Ok(())
}

#[given("hosting environment variables describe an ACP custom agent")]
fn hosting_environment_variables_describe_an_acp_custom_agent(
    hosting_config_loader_state: &HostingConfigLoaderState,
) -> StepResult<()> {
    set_env_var(
        hosting_config_loader_state,
        "PODBOT_WORKSPACE_SOURCE",
        "host_mount",
    )?;
    set_env_var(
        hosting_config_loader_state,
        "PODBOT_WORKSPACE_HOST_PATH",
        "/tmp/project",
    )?;
    set_env_var(hosting_config_loader_state, "PODBOT_AGENT_KIND", "custom")?;
    set_env_var(hosting_config_loader_state, "PODBOT_AGENT_MODE", "acp")?;
    set_env_var(
        hosting_config_loader_state,
        "PODBOT_AGENT_COMMAND",
        "opencode",
    )?;
    Ok(())
}

#[given("the hosting loader uses run intent")]
fn the_hosting_loader_uses_run_intent(
    hosting_config_loader_state: &HostingConfigLoaderState,
) -> StepResult<()> {
    let mut options = get_options(hosting_config_loader_state)?;
    options.command_intent = CommandIntent::Run;
    hosting_config_loader_state.options.set(options);
    Ok(())
}

#[when("the hosting library configuration is loaded")]
fn the_hosting_library_configuration_is_loaded(
    hosting_config_loader_state: &HostingConfigLoaderState,
) -> StepResult<()> {
    let env = create_mock_env(hosting_config_loader_state)?;
    let options = get_options(hosting_config_loader_state)?;

    match load_config_with_env(&env, &options) {
        Ok(config) => hosting_config_loader_state.config.set(config),
        Err(error) => hosting_config_loader_state.error.set(error.to_string()),
    }

    Ok(())
}

#[then("the loaded hosting configuration uses a host-mounted workspace")]
fn the_loaded_hosting_configuration_uses_a_host_mounted_workspace(
    hosting_config_loader_state: &HostingConfigLoaderState,
) -> StepResult<()> {
    let config = get_loaded_config(hosting_config_loader_state)?;
    assert_eq!(config.workspace.source, WorkspaceSource::HostMount);
    Ok(())
}

#[then("the loaded hosting agent mode is codex_app_server")]
fn the_loaded_hosting_agent_mode_is_codex_app_server(
    hosting_config_loader_state: &HostingConfigLoaderState,
) -> StepResult<()> {
    assert_agent_mode(hosting_config_loader_state, "codex_app_server")
}

#[then("the loaded hosting agent mode is acp")]
fn the_loaded_hosting_agent_mode_is_acp(
    hosting_config_loader_state: &HostingConfigLoaderState,
) -> StepResult<()> {
    assert_agent_mode(hosting_config_loader_state, "acp")
}

#[then("the loaded hosting workspace container path is /workspace")]
fn the_loaded_hosting_workspace_container_path_is_workspace(
    hosting_config_loader_state: &HostingConfigLoaderState,
) -> StepResult<()> {
    let config = get_loaded_config(hosting_config_loader_state)?;
    assert_eq!(config.workspace.container_path, Some("/workspace".into()));
    Ok(())
}

#[then("hosting configuration loading fails mentioning podbot host")]
fn hosting_configuration_loading_fails_mentioning_podbot_host(
    hosting_config_loader_state: &HostingConfigLoaderState,
) -> StepResult<()> {
    let error = hosting_config_loader_state
        .error
        .get()
        .ok_or_else(|| String::from("error should be set"))?;
    assert!(
        error.contains("podbot host"),
        "expected error to mention podbot host, got: {error}"
    );
    Ok(())
}

fn get_env_vars(hosting_config_loader_state: &HostingConfigLoaderState) -> StepResult<EnvVars> {
    hosting_config_loader_state
        .env_vars
        .get()
        .ok_or_else(|| String::from("env vars should be initialised"))
}

fn set_env_var(
    hosting_config_loader_state: &HostingConfigLoaderState,
    key: &str,
    value: &str,
) -> StepResult<()> {
    let env_vars = get_env_vars(hosting_config_loader_state)?;
    let mut vars = env_vars
        .lock()
        .map_err(|_| String::from("mutex poisoned"))?;
    vars.insert(String::from(key), String::from(value));
    Ok(())
}

fn create_mock_env(hosting_config_loader_state: &HostingConfigLoaderState) -> StepResult<MockEnv> {
    let env_vars = get_env_vars(hosting_config_loader_state)?;
    let vars = env_vars
        .lock()
        .map_err(|_| String::from("mutex poisoned"))?
        .clone();

    let mut env = MockEnv::new();
    env.expect_string()
        .returning(move |key| vars.get(key).cloned());
    Ok(env)
}

fn get_options(
    hosting_config_loader_state: &HostingConfigLoaderState,
) -> StepResult<ConfigLoadOptions> {
    hosting_config_loader_state
        .options
        .get()
        .ok_or_else(|| String::from("options should be set"))
}

fn write_config_file(
    hosting_config_loader_state: &HostingConfigLoaderState,
    content: &str,
) -> StepResult<Utf8PathBuf> {
    let temp_dir = tempfile::TempDir::new().map_err(|error| error.to_string())?;
    let temp_dir_arc = Arc::new(temp_dir);
    let raw_path = temp_dir_arc.path().join("config.toml");
    std::fs::write(&raw_path, content).map_err(|error| error.to_string())?;
    let path = Utf8PathBuf::try_from(raw_path).map_err(|error| error.to_string())?;
    hosting_config_loader_state.temp_dir.set(temp_dir_arc);
    Ok(path)
}

fn get_loaded_config(
    hosting_config_loader_state: &HostingConfigLoaderState,
) -> StepResult<podbot::config::AppConfig> {
    hosting_config_loader_state
        .config
        .get()
        .ok_or_else(|| String::from("config should be set"))
}

fn assert_agent_mode(
    hosting_config_loader_state: &HostingConfigLoaderState,
    expected: &str,
) -> StepResult<()> {
    let config = get_loaded_config(hosting_config_loader_state)?;
    let actual = serde_json::to_string(&config.agent.mode).map_err(|error| error.to_string())?;
    let expected_json = format!("\"{expected}\"");
    if actual != expected_json {
        return Err(format!("expected agent mode {expected_json}, got {actual}"));
    }
    Ok(())
}
