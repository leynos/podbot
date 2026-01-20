//! Configuration loading with layered precedence.
//!
//! This module provides functions to load configuration with the precedence order
//! (lowest to highest): application defaults, configuration file, environment
//! variables, command-line arguments.
//!
//! The loader bridges between `ortho_config` and clap-based CLI parsing, using
//! `MergeComposer` to handle file and environment layers while allowing the main
//! CLI parser to handle subcommands.

use std::path::PathBuf;

use camino::Utf8PathBuf;
use ortho_config::discovery::ConfigDiscovery;
use ortho_config::serde_json;
use ortho_config::{MergeComposer, toml};

use crate::config::{AppConfig, Cli};
use crate::error::{ConfigError, Result};

/// Load a configuration file and push it to the composer.
fn load_config_file(path: &PathBuf, composer: &mut MergeComposer) -> Result<()> {
    let content = std::fs::read_to_string(path).map_err(|e| ConfigError::ParseError {
        message: format!("failed to read {}: {e}", path.display()),
    })?;

    let value =
        toml::from_str::<serde_json::Value>(&content).map_err(|e| ConfigError::ParseError {
            message: format!("failed to parse {}: {e}", path.display()),
        })?;

    let utf8_path = Utf8PathBuf::try_from(path.clone()).ok();
    composer.push_file(value, utf8_path);
    Ok(())
}

/// Load configuration with full layer precedence.
///
/// This function loads configuration from all available sources:
/// 1. Application defaults defined in the struct
/// 2. Configuration file (discovered via XDG paths or `PODBOT_CONFIG_PATH`)
/// 3. Environment variables prefixed with `PODBOT_`
/// 4. Command-line arguments (from the provided `Cli`)
///
/// Later sources override earlier ones.
///
/// # Errors
///
/// Returns `ConfigError` if configuration loading fails due to:
/// - Malformed configuration files
/// - Invalid environment variable values
/// - Missing required fields after merge
pub fn load_config(cli: &Cli) -> Result<AppConfig> {
    let mut composer = MergeComposer::new();

    // Layer 1: Defaults (serialised from AppConfig::default()).
    let defaults =
        serde_json::to_value(AppConfig::default()).map_err(|e| ConfigError::ParseError {
            message: format!("failed to serialise defaults: {e}"),
        })?;
    composer.push_defaults(defaults);

    // Layer 2: Configuration file.
    // Use the CLI-provided path (if it exists), or discover via XDG paths.
    let config_path: Option<PathBuf> = cli
        .config
        .as_ref()
        .map(|p| p.as_std_path().to_owned())
        .filter(|p| p.exists());
    let discovered_path = config_path.or_else(|| {
        // Discover config files using ortho_config's ConfigDiscovery builder.
        let discovery = ConfigDiscovery::builder("podbot")
            .env_var("PODBOT_CONFIG_PATH")
            .config_file_name("config.toml")
            .dotfile_name(".podbot.toml")
            .build();
        discovery.candidates().into_iter().find(|p| p.exists())
    });

    if let Some(path) = discovered_path {
        load_config_file(&path, &mut composer)?;
    }

    // Layer 3: Environment variables.
    let env_values = collect_env_vars();
    if !env_values.is_null() {
        composer.push_environment(env_values);
    }

    // Layer 4: CLI overrides.
    let cli_overrides = build_cli_overrides(cli);
    if !cli_overrides.is_null() {
        composer.push_cli(cli_overrides);
    }

    // Merge all layers into the final configuration.
    let config =
        AppConfig::merge_from_layers(composer.layers()).map_err(ConfigError::OrthoConfig)?;

    Ok(config)
}

/// Collect environment variables with the `PODBOT_` prefix into a JSON value.
fn collect_env_vars() -> serde_json::Value {
    let mut map = serde_json::Map::new();

    // Top-level fields.
    collect_top_level_env_vars(&mut map);

    // Nested config sections.
    collect_github_env_vars(&mut map);
    collect_sandbox_env_vars(&mut map);
    collect_agent_env_vars(&mut map);
    collect_workspace_env_vars(&mut map);
    collect_creds_env_vars(&mut map);

    if map.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Object(map)
    }
}

/// Collect top-level environment variables (`engine_socket`, `image`).
fn collect_top_level_env_vars(map: &mut serde_json::Map<String, serde_json::Value>) {
    if let Ok(val) = std::env::var("PODBOT_ENGINE_SOCKET") {
        map.insert("engine_socket".to_owned(), serde_json::Value::String(val));
    }
    if let Ok(val) = std::env::var("PODBOT_IMAGE") {
        map.insert("image".to_owned(), serde_json::Value::String(val));
    }
}

/// Collect GitHub-related environment variables.
fn collect_github_env_vars(map: &mut serde_json::Map<String, serde_json::Value>) {
    let mut github = serde_json::Map::new();
    if let Ok(val) = std::env::var("PODBOT_GITHUB_APP_ID") {
        if let Ok(id) = val.parse::<u64>() {
            github.insert("app_id".to_owned(), serde_json::Value::Number(id.into()));
        }
    }
    if let Ok(val) = std::env::var("PODBOT_GITHUB_INSTALLATION_ID") {
        if let Ok(id) = val.parse::<u64>() {
            github.insert(
                "installation_id".to_owned(),
                serde_json::Value::Number(id.into()),
            );
        }
    }
    if let Ok(val) = std::env::var("PODBOT_GITHUB_PRIVATE_KEY_PATH") {
        github.insert(
            "private_key_path".to_owned(),
            serde_json::Value::String(val),
        );
    }
    if !github.is_empty() {
        map.insert("github".to_owned(), serde_json::Value::Object(github));
    }
}

/// Collect sandbox-related environment variables.
fn collect_sandbox_env_vars(map: &mut serde_json::Map<String, serde_json::Value>) {
    let mut sandbox = serde_json::Map::new();
    if let Ok(val) = std::env::var("PODBOT_SANDBOX_PRIVILEGED") {
        if let Ok(b) = val.parse::<bool>() {
            sandbox.insert("privileged".to_owned(), serde_json::Value::Bool(b));
        }
    }
    if let Ok(val) = std::env::var("PODBOT_SANDBOX_MOUNT_DEV_FUSE") {
        if let Ok(b) = val.parse::<bool>() {
            sandbox.insert("mount_dev_fuse".to_owned(), serde_json::Value::Bool(b));
        }
    }
    if !sandbox.is_empty() {
        map.insert("sandbox".to_owned(), serde_json::Value::Object(sandbox));
    }
}

/// Collect agent-related environment variables.
fn collect_agent_env_vars(map: &mut serde_json::Map<String, serde_json::Value>) {
    let mut agent = serde_json::Map::new();
    if let Ok(val) = std::env::var("PODBOT_AGENT_KIND") {
        agent.insert("kind".to_owned(), serde_json::Value::String(val));
    }
    if let Ok(val) = std::env::var("PODBOT_AGENT_MODE") {
        agent.insert("mode".to_owned(), serde_json::Value::String(val));
    }
    if !agent.is_empty() {
        map.insert("agent".to_owned(), serde_json::Value::Object(agent));
    }
}

/// Collect workspace-related environment variables.
fn collect_workspace_env_vars(map: &mut serde_json::Map<String, serde_json::Value>) {
    if let Ok(val) = std::env::var("PODBOT_WORKSPACE_BASE_DIR") {
        let mut workspace = serde_json::Map::new();
        workspace.insert("base_dir".to_owned(), serde_json::Value::String(val));
        map.insert("workspace".to_owned(), serde_json::Value::Object(workspace));
    }
}

/// Collect credentials-related environment variables.
fn collect_creds_env_vars(map: &mut serde_json::Map<String, serde_json::Value>) {
    let mut creds = serde_json::Map::new();
    if let Ok(val) = std::env::var("PODBOT_CREDS_COPY_CLAUDE") {
        if let Ok(b) = val.parse::<bool>() {
            creds.insert("copy_claude".to_owned(), serde_json::Value::Bool(b));
        }
    }
    if let Ok(val) = std::env::var("PODBOT_CREDS_COPY_CODEX") {
        if let Ok(b) = val.parse::<bool>() {
            creds.insert("copy_codex".to_owned(), serde_json::Value::Bool(b));
        }
    }
    if !creds.is_empty() {
        map.insert("creds".to_owned(), serde_json::Value::Object(creds));
    }
}

/// Build a JSON value containing CLI overrides.
fn build_cli_overrides(cli: &Cli) -> serde_json::Value {
    let mut overrides = serde_json::Map::new();

    if let Some(ref socket) = cli.engine_socket {
        overrides.insert(
            "engine_socket".to_owned(),
            serde_json::Value::String(socket.clone()),
        );
    }

    if let Some(ref image) = cli.image {
        overrides.insert("image".to_owned(), serde_json::Value::String(image.clone()));
    }

    if overrides.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Object(overrides)
    }
}
