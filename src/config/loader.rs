//! Configuration loading with layered precedence.
//!
//! This module provides functions to load configuration with the precedence order
//! (lowest to highest): application defaults, configuration file, environment
//! variables, host overrides.
//!
//! # Architecture Note: Why Manual Layer Composition?
//!
//! The `OrthoConfig` derive macro provides `load()` and `compose_layers()` methods
//! that handle discovery, environment variables, and CLI parsing automatically.
//! However, this loader uses `MergeComposer` manually because:
//!
//! 1. **Subcommand separation**: The CLI (`Cli` struct) handles subcommand dispatch
//!    via clap's `#[command(subcommand)]`, while `AppConfig` holds configuration
//!    values. `OrthoConfig`'s `load()` expects to own the entire CLI parsing.
//!
//! 2. **Environment variable validation**: `OrthoConfig`'s environment layer uses
//!    Figment, which silently ignores unparseable values. This loader implements
//!    fail-fast validation that returns errors for invalid typed values.
//!
//! 3. **Custom discovery integration**: Hosts (including the `podbot` CLI) can
//!    provide an explicit config-path hint, which must be honoured before falling
//!    back to discovery candidates.
//!
//! The trade-off is more code in this module, but better error messages and
//! integration with the existing CLI structure.
//!
//! # Environment Variable Handling
//!
//! Environment variables with unparseable values (e.g., `PODBOT_SANDBOX_PRIVILEGED=maybe`
//! instead of `true`/`false`) return an error immediately. This fail-fast approach
//! ensures misconfigurations are visible to users rather than silently falling back
//! to defaults.
//!
//! String fields (e.g., `PODBOT_ENGINE_SOCKET`) are always accepted. Typed fields
//! like booleans (`PODBOT_SANDBOX_PRIVILEGED`) or integers (`PODBOT_GITHUB_APP_ID`)
//! must have valid values or the configuration loading will fail with a clear error.

use camino::Utf8PathBuf;
use cap_std::ambient_authority;
use cap_std::fs_utf8::Dir;
use ortho_config::discovery::ConfigDiscovery;
use ortho_config::serde_json;
use ortho_config::{MergeComposer, toml};

use crate::config::env_vars::collect_env_vars;
use crate::config::{AppConfig, ConfigLoadOptions, ConfigOverrides};
use crate::error::{ConfigError, Result};

/// Load a configuration file and push it to the composer.
///
/// Uses `cap_std::fs_utf8` for capability-oriented filesystem access as per
/// project conventions. The function opens the parent directory of the config
/// file and reads from there.
fn load_config_file(path: &Utf8PathBuf, composer: &mut MergeComposer) -> Result<()> {
    // Open the parent directory using ambient authority, then read the file.
    let current_dir = Utf8PathBuf::from(".");
    let parent = path.parent().unwrap_or_else(|| current_dir.as_ref());
    let file_name = path.file_name().unwrap_or(path.as_str());

    let dir = Dir::open_ambient_dir(parent, ambient_authority()).map_err(|e| {
        ConfigError::ParseError {
            message: format!("failed to open directory {parent}: {e}"),
        }
    })?;

    let content = dir
        .read_to_string(file_name)
        .map_err(|e| ConfigError::ParseError {
            message: format!("failed to read {path}: {e}"),
        })?;

    let value =
        toml::from_str::<serde_json::Value>(&content).map_err(|e| ConfigError::ParseError {
            message: format!("failed to parse {path}: {e}"),
        })?;

    composer.push_file(value, Some(path.clone()));
    Ok(())
}

/// Load configuration with full layer precedence.
///
/// This function loads configuration from all available sources:
/// 1. Application defaults defined in the struct
/// 2. Configuration file (discovered via XDG paths or `PODBOT_CONFIG_PATH`)
/// 3. Environment variables prefixed with `PODBOT_`
/// 4. Host-supplied overrides (for example CLI flags)
///
/// Later sources override earlier ones.
///
/// # Errors
///
/// Returns `ConfigError` if configuration loading fails due to:
/// - Malformed configuration files
/// - Invalid typed environment variable values (e.g., non-boolean for
///   `PODBOT_SANDBOX_PRIVILEGED`)
/// - Missing required fields after merge
pub fn load_config(options: &ConfigLoadOptions) -> Result<AppConfig> {
    let env = mockable::DefaultEnv::new();
    load_config_with_env(&env, options)
}

/// Load configuration with full layer precedence using an injected environment.
///
/// This function enables deterministic unit and behavioural testing without
/// mutating the process environment.
///
/// # Errors
///
/// Returns `ConfigError` if configuration loading fails due to:
/// - Malformed configuration files
/// - Invalid typed environment variable values (e.g., non-boolean for
///   `PODBOT_SANDBOX_PRIVILEGED`)
/// - `OrthoConfig` merge failures after layer composition
pub fn load_config_with_env<E: mockable::Env>(
    env: &E,
    options: &ConfigLoadOptions,
) -> Result<AppConfig> {
    let mut composer = MergeComposer::new();

    // Layer 1: Defaults (serialised from AppConfig::default()).
    let defaults =
        serde_json::to_value(AppConfig::default()).map_err(|e| ConfigError::ParseError {
            message: format!("failed to serialise defaults: {e}"),
        })?;
    composer.push_defaults(defaults);

    // Layer 2: Configuration file.
    let config_path = resolve_config_path(env, options);

    if let Some(ref path) = config_path {
        load_config_file(path, &mut composer)?;
    }

    // Layer 3: Environment variables.
    let env_values = collect_env_vars(env)?;
    if !env_values.is_null() {
        composer.push_environment(env_values);
    }

    // Layer 4: host overrides.
    let overrides = build_overrides(&options.overrides);
    if !overrides.is_null() {
        composer.push_cli(overrides);
    }

    // Merge all layers into the final configuration.
    let config =
        AppConfig::merge_from_layers(composer.layers()).map_err(ConfigError::OrthoConfig)?;

    Ok(config)
}

fn resolve_config_path<E: mockable::Env>(
    env: &E,
    options: &ConfigLoadOptions,
) -> Option<Utf8PathBuf> {
    options.config_path_hint.clone().map_or_else(
        || env_path(env).or_else(|| discover_config_path(options.discover_config)),
        |hint| {
            hint.exists()
                .then_some(hint)
                .or_else(|| discover_config_path(options.discover_config))
        },
    )
}

fn env_path<E: mockable::Env>(env: &E) -> Option<Utf8PathBuf> {
    env.string("PODBOT_CONFIG_PATH").and_then(|raw| {
        let path = Utf8PathBuf::from(raw);
        path.exists().then_some(path)
    })
}

fn discover_config_path(enabled: bool) -> Option<Utf8PathBuf> {
    if !enabled {
        return None;
    }

    let discovery = ConfigDiscovery::builder("podbot")
        .config_file_name("config.toml")
        .dotfile_name(".podbot.toml")
        .build();
    discovery
        .candidates()
        .into_iter()
        .filter(|p| p.exists())
        .find_map(|p| Utf8PathBuf::try_from(p).ok())
}

/// Build a JSON value containing host overrides.
fn build_overrides(overrides: &ConfigOverrides) -> serde_json::Value {
    if overrides.is_empty() {
        return serde_json::Value::Null;
    }

    let mut json_overrides = serde_json::Map::new();

    if let Some(ref socket) = overrides.engine_socket {
        json_overrides.insert(
            "engine_socket".to_owned(),
            serde_json::Value::String(socket.clone()),
        );
    }

    if let Some(ref image) = overrides.image {
        json_overrides.insert("image".to_owned(), serde_json::Value::String(image.clone()));
    }

    serde_json::Value::Object(json_overrides)
}
