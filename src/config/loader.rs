//! Configuration loading with layered precedence.
//!
//! This module provides functions to load configuration with the precedence order
//! (lowest to highest): application defaults, configuration file, environment
//! variables, command-line arguments.
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
//! 3. **Custom discovery integration**: The `Cli` struct already accepts `--config`
//!    via clap, so discovery must honour that path before falling back to XDG paths.
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
use ortho_config::serde_json::{self, Map, Value};
use ortho_config::{MergeComposer, toml};

use crate::config::{AppConfig, ConfigLoadOptions, ConfigOverrides};
use crate::error::{ConfigError, Result};

// ============================================================================
// Environment Variable Specification Table
// ============================================================================

/// The type of value expected from an environment variable.
#[derive(Clone, Copy)]
enum EnvVarType {
    /// String value (always accepted).
    String,
    /// Boolean value (`true`/`false`). Invalid values return an error.
    Bool,
    /// Unsigned 64-bit integer. Invalid values return an error.
    U64,
}

/// Specification for a single environment variable mapping.
struct EnvVarSpec {
    /// The environment variable name (e.g., `PODBOT_ENGINE_SOCKET`).
    env_var: &'static str,
    /// The JSON path segments (e.g., `["sandbox", "privileged"]`).
    path: &'static [&'static str],
    /// The expected value type.
    var_type: EnvVarType,
}

/// Table of all environment variables and their JSON paths.
///
/// Adding or modifying environment variable mappings is a single-line change here.
/// The order doesn't matter as the table is processed in a single pass.
const ENV_VAR_SPECS: &[EnvVarSpec] = &[
    // Top-level fields
    EnvVarSpec {
        env_var: "PODBOT_ENGINE_SOCKET",
        path: &["engine_socket"],
        var_type: EnvVarType::String,
    },
    EnvVarSpec {
        env_var: "PODBOT_IMAGE",
        path: &["image"],
        var_type: EnvVarType::String,
    },
    // GitHub fields
    EnvVarSpec {
        env_var: "PODBOT_GITHUB_APP_ID",
        path: &["github", "app_id"],
        var_type: EnvVarType::U64,
    },
    EnvVarSpec {
        env_var: "PODBOT_GITHUB_INSTALLATION_ID",
        path: &["github", "installation_id"],
        var_type: EnvVarType::U64,
    },
    EnvVarSpec {
        env_var: "PODBOT_GITHUB_PRIVATE_KEY_PATH",
        path: &["github", "private_key_path"],
        var_type: EnvVarType::String,
    },
    // Sandbox fields
    EnvVarSpec {
        env_var: "PODBOT_SANDBOX_PRIVILEGED",
        path: &["sandbox", "privileged"],
        var_type: EnvVarType::Bool,
    },
    EnvVarSpec {
        env_var: "PODBOT_SANDBOX_MOUNT_DEV_FUSE",
        path: &["sandbox", "mount_dev_fuse"],
        var_type: EnvVarType::Bool,
    },
    EnvVarSpec {
        env_var: "PODBOT_SANDBOX_SELINUX_LABEL_MODE",
        path: &["sandbox", "selinux_label_mode"],
        var_type: EnvVarType::String,
    },
    // Agent fields
    EnvVarSpec {
        env_var: "PODBOT_AGENT_KIND",
        path: &["agent", "kind"],
        var_type: EnvVarType::String,
    },
    EnvVarSpec {
        env_var: "PODBOT_AGENT_MODE",
        path: &["agent", "mode"],
        var_type: EnvVarType::String,
    },
    // Workspace fields
    EnvVarSpec {
        env_var: "PODBOT_WORKSPACE_BASE_DIR",
        path: &["workspace", "base_dir"],
        var_type: EnvVarType::String,
    },
    // Creds fields
    EnvVarSpec {
        env_var: "PODBOT_CREDS_COPY_CLAUDE",
        path: &["creds", "copy_claude"],
        var_type: EnvVarType::Bool,
    },
    EnvVarSpec {
        env_var: "PODBOT_CREDS_COPY_CODEX",
        path: &["creds", "copy_codex"],
        var_type: EnvVarType::Bool,
    },
];

/// Returns the list of environment variable names recognised by the config loader.
///
/// This is primarily useful for tests that need to clear all `PODBOT_*` environment
/// variables to ensure isolation. Using this function instead of a hard-coded list
/// ensures the test stays in sync with the loader's actual environment variable
/// mappings.
#[must_use]
pub fn env_var_names() -> Vec<&'static str> {
    ENV_VAR_SPECS.iter().map(|spec| spec.env_var).collect()
}

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
    // Use the host-provided path hint (if it exists), then PODBOT_CONFIG_PATH,
    // then discover via XDG paths.
    let config_path: Option<Utf8PathBuf> = options
        .config_path_hint
        .clone()
        .filter(|p| p.exists())
        .or_else(|| {
            env.string("PODBOT_CONFIG_PATH").and_then(|raw| {
                let path = Utf8PathBuf::from(raw);
                path.exists().then_some(path)
            })
        })
        .or_else(|| {
            if !options.discover_config {
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
        });

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

/// Collect environment variables with the `PODBOT_` prefix into a JSON value.
///
/// This function uses a data-driven approach: all environment variable mappings
/// are defined in [`ENV_VAR_SPECS`]. Adding or changing mappings requires only
/// a single-line change in that table.
///
/// # Errors
///
/// Returns `ConfigError::InvalidValue` if a typed environment variable (bool, u64)
/// has an unparseable value. This fail-fast approach ensures misconfigurations are
/// visible to users.
fn collect_env_vars<E: mockable::Env>(env: &E) -> Result<Value> {
    let mut root = Map::new();

    for spec in ENV_VAR_SPECS {
        let Some(raw_value) = env.string(spec.env_var) else {
            continue;
        };

        // Parse the value according to its expected type.
        // Invalid values return an error immediately (fail-fast).
        let json_value = match spec.var_type {
            EnvVarType::String => Value::String(raw_value),
            EnvVarType::Bool => match raw_value.parse::<bool>() {
                Ok(b) => Value::Bool(b),
                Err(_) => {
                    return Err(ConfigError::InvalidValue {
                        field: spec.env_var.to_owned(),
                        reason: format!("expected bool (true/false), got '{raw_value}'"),
                    }
                    .into());
                }
            },
            EnvVarType::U64 => match raw_value.parse::<u64>() {
                Ok(n) => Value::Number(n.into()),
                Err(_) => {
                    return Err(ConfigError::InvalidValue {
                        field: spec.env_var.to_owned(),
                        reason: format!("expected unsigned integer, got '{raw_value}'"),
                    }
                    .into());
                }
            },
        };

        // Insert at the appropriate path (supports arbitrary nesting depth).
        insert_at_path(&mut root, spec.path, json_value);
    }

    if root.is_empty() {
        Ok(Value::Null)
    } else {
        Ok(Value::Object(root))
    }
}

/// Insert a value at a nested path in a JSON map.
///
/// For a path like `["sandbox", "privileged"]`, this creates the intermediate
/// `sandbox` object if needed and inserts `privileged` within it.
fn insert_at_path(root: &mut Map<String, Value>, path: &[&str], value: Value) {
    let Some((&field, parents)) = path.split_last() else {
        return;
    };

    // Navigate to the parent object, creating intermediate objects as needed.
    let mut current = root;
    for &segment in parents {
        // Ensure the entry is an object; if it's not (shouldn't happen with our
        // controlled path specs), skip this insertion.
        let entry = current
            .entry(segment.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        let Some(obj) = entry.as_object_mut() else {
            return;
        };
        current = obj;
    }

    // Insert the final field.
    current.insert(field.to_owned(), value);
}

/// Build a JSON value containing host overrides.
fn build_overrides(overrides: &ConfigOverrides) -> serde_json::Value {
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

    if json_overrides.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Object(json_overrides)
    }
}
