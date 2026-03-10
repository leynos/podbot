//! Environment-variable configuration layer.
//!
//! This module defines the mapping from `PODBOT_*` environment variables into a
//! JSON structure compatible with the `ortho_config` merge composer.

use ortho_config::serde_json::{Map, Value};

use crate::error::{ConfigError, Result};

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
/// Adding or modifying environment variable mappings is a single-line change
/// here. The order doesn't matter as the table is processed in a single pass.
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

/// Returns the list of environment variable names recognised by the config
/// loader.
///
/// This includes the variables mapped in this module plus
/// `PODBOT_CONFIG_PATH`, which is consumed by config-path discovery.
///
/// This is primarily useful for tests that need to clear all recognised
/// `PODBOT_*` environment variables to ensure isolation. Using this function
/// instead of a hard-coded list keeps tests in sync with loader behaviour.
#[must_use]
pub fn env_var_names() -> Vec<&'static str> {
    ENV_VAR_SPECS
        .iter()
        .map(|spec| spec.env_var)
        .chain(std::iter::once("PODBOT_CONFIG_PATH"))
        .collect()
}

/// Collect environment variables with the `PODBOT_` prefix into a JSON value.
///
/// This function uses a data-driven approach: all environment variable mappings
/// are defined in [`ENV_VAR_SPECS`]. Adding or changing mappings requires only a
/// single-line change in that table.
///
/// # Errors
///
/// Returns `ConfigError::InvalidValue` if a typed environment variable (bool,
/// u64) has an unparseable value. This fail-fast approach ensures
/// misconfigurations are visible to users.
pub(crate) fn collect_env_vars<E: mockable::Env>(env: &E) -> Result<Value> {
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
