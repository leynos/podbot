//! Behavioural test helpers for the library-facing configuration loader.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use camino::Utf8PathBuf;
use mockable::MockEnv;
use podbot::config::{ConfigLoadOptions, ConfigOverrides, load_config_with_env};
use podbot::error::PodbotError;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, then, when};

/// Step result type for BDD tests.
///
/// `rstest-bdd` step functions return `Result` values so failures are surfaced
/// as scenario errors without panicking.
pub type StepResult<T> = Result<T, String>;

type EnvVars = Arc<Mutex<HashMap<String, String>>>;

/// State shared across configuration loader scenarios.
#[derive(Default, ScenarioState)]
pub struct ConfigLoaderState {
    env_vars: Slot<EnvVars>,
    temp_dir: Slot<Arc<tempfile::TempDir>>,
    options: Slot<ConfigLoadOptions>,
    config: Slot<podbot::config::AppConfig>,
    error: Slot<String>,
}

/// Fixture providing a fresh configuration loader state.
#[fixture]
pub fn config_loader_state() -> ConfigLoaderState {
    let state = ConfigLoaderState::default();
    state.env_vars.set(Arc::new(Mutex::new(HashMap::new())));
    state.options.set(ConfigLoadOptions {
        discover_config: false,
        ..ConfigLoadOptions::default()
    });
    state
}

fn get_env_vars(config_loader_state: &ConfigLoaderState) -> StepResult<EnvVars> {
    config_loader_state
        .env_vars
        .get()
        .ok_or_else(|| String::from("env_vars should be initialised"))
}

fn set_env_var(config_loader_state: &ConfigLoaderState, key: &str, value: &str) -> StepResult<()> {
    let env_vars = get_env_vars(config_loader_state)?;
    let mut vars = env_vars
        .lock()
        .map_err(|_| String::from("mutex poisoned"))?;
    vars.insert(String::from(key), String::from(value));
    Ok(())
}

fn create_mock_env(config_loader_state: &ConfigLoaderState) -> StepResult<MockEnv> {
    let env_vars = get_env_vars(config_loader_state)?;
    let vars = env_vars
        .lock()
        .map_err(|_| String::from("mutex poisoned"))?
        .clone();

    let mut env = MockEnv::new();
    env.expect_string()
        .returning(move |key| vars.get(key).cloned());
    Ok(env)
}

fn get_options(config_loader_state: &ConfigLoaderState) -> StepResult<ConfigLoadOptions> {
    config_loader_state
        .options
        .get()
        .ok_or_else(|| String::from("options should be set"))
}

fn set_options(config_loader_state: &ConfigLoaderState, options: ConfigLoadOptions) {
    config_loader_state.options.set(options);
}

fn set_config(config_loader_state: &ConfigLoaderState, config: podbot::config::AppConfig) {
    config_loader_state.config.set(config);
}

fn set_error(config_loader_state: &ConfigLoaderState, error: &PodbotError) {
    config_loader_state.error.set(error.to_string());
}

fn get_loaded_config(
    config_loader_state: &ConfigLoaderState,
) -> StepResult<podbot::config::AppConfig> {
    config_loader_state
        .config
        .get()
        .ok_or_else(|| String::from("config should be set"))
}

fn get_error(config_loader_state: &ConfigLoaderState) -> StepResult<String> {
    config_loader_state
        .error
        .get()
        .ok_or_else(|| String::from("error should be set"))
}

fn write_config_file(
    config_loader_state: &ConfigLoaderState,
    file_name: &str,
    content: &str,
) -> StepResult<Utf8PathBuf> {
    let temp_dir = tempfile::TempDir::new().map_err(|e| e.to_string())?;
    let temp_dir_arc = Arc::new(temp_dir);
    let raw_path = temp_dir_arc.path().join(file_name);
    std::fs::write(&raw_path, content).map_err(|e| e.to_string())?;

    let path = Utf8PathBuf::try_from(raw_path).map_err(|e| e.to_string())?;
    config_loader_state.temp_dir.set(temp_dir_arc);
    Ok(path)
}

#[given("no configuration sources are provided")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions return StepResult for consistency"
)]
fn no_configuration_sources_are_provided(
    config_loader_state: &ConfigLoaderState,
) -> StepResult<()> {
    set_options(
        config_loader_state,
        ConfigLoadOptions {
            discover_config: false,
            ..ConfigLoadOptions::default()
        },
    );
    Ok(())
}

#[given("a configuration file sets image to {image}")]
fn a_configuration_file_sets_image_to(
    config_loader_state: &ConfigLoaderState,
    image: String,
) -> StepResult<()> {
    let content = format!("image = \"{image}\"\n");
    let config_path_hint = write_config_file(config_loader_state, "config.toml", &content)?;
    set_options(
        config_loader_state,
        ConfigLoadOptions {
            config_path_hint: Some(config_path_hint),
            discover_config: false,
            ..ConfigLoadOptions::default()
        },
    );
    Ok(())
}

#[given("the environment variable {name} is set to {value}")]
fn the_environment_variable_is_set_to(
    config_loader_state: &ConfigLoaderState,
    name: String,
    value: String,
) -> StepResult<()> {
    set_env_var(config_loader_state, &name, &value)?;
    Ok(())
}

#[given("host overrides set image to {image}")]
fn host_overrides_set_image_to(
    config_loader_state: &ConfigLoaderState,
    image: String,
) -> StepResult<()> {
    let mut options = get_options(config_loader_state)?;
    options.overrides = ConfigOverrides {
        engine_socket: None,
        image: Some(image),
    };
    set_options(config_loader_state, options);
    Ok(())
}

#[when("the library configuration is loaded")]
fn the_library_configuration_is_loaded(config_loader_state: &ConfigLoaderState) -> StepResult<()> {
    let env = create_mock_env(config_loader_state)?;
    let options = get_options(config_loader_state)?;

    match load_config_with_env(&env, &options) {
        Ok(config) => set_config(config_loader_state, config),
        Err(error) => set_error(config_loader_state, &error),
    }

    Ok(())
}

#[then("the loaded configuration uses defaults")]
fn the_loaded_configuration_uses_defaults(
    config_loader_state: &ConfigLoaderState,
) -> StepResult<()> {
    let config = get_loaded_config(config_loader_state)?;
    assert!(
        config.engine_socket.is_none(),
        "engine_socket should be None"
    );
    assert!(config.image.is_none(), "image should be None");
    assert!(
        !config.sandbox.privileged,
        "sandbox.privileged should be false"
    );
    Ok(())
}

#[then("the loaded configuration image is {image}")]
fn the_loaded_configuration_image_is(
    config_loader_state: &ConfigLoaderState,
    image: String,
) -> StepResult<()> {
    let config = get_loaded_config(config_loader_state)?;
    let actual = config
        .image
        .ok_or_else(|| String::from("image should be set"))?;
    assert_eq!(actual, image, "resolved image mismatch");
    Ok(())
}

#[then("configuration loading fails mentioning {text}")]
fn configuration_loading_fails_mentioning(
    config_loader_state: &ConfigLoaderState,
    text: String,
) -> StepResult<()> {
    let error = get_error(config_loader_state)?;
    assert!(
        error.contains(&text),
        "expected error to mention '{text}', got: {error}"
    );
    Ok(())
}
