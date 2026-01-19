//! Configuration data types for podbot.

use camino::Utf8PathBuf;
use clap::ValueEnum;
use ortho_config::{OrthoConfig, OrthoResult, PostMergeContext, PostMergeHook};
use serde::{Deserialize, Serialize};

/// The kind of AI agent to run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    /// Claude Code agent.
    #[default]
    Claude,
    /// `OpenAI` Codex agent.
    Codex,
}

/// The execution mode for the agent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Run the agent in podbot-managed mode.
    #[default]
    Podbot,
}

/// `GitHub` App configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GitHubConfig {
    /// The `GitHub` App ID.
    pub app_id: Option<u64>,

    /// The `GitHub` App installation ID.
    pub installation_id: Option<u64>,

    /// Path to the `GitHub` App private key file.
    pub private_key_path: Option<Utf8PathBuf>,
}

impl GitHubConfig {
    /// Validates that all required `GitHub` fields are present and non-zero.
    ///
    /// This method checks that `app_id`, `installation_id`, and `private_key_path`
    /// are all set and that numeric IDs are non-zero. Call this before performing
    /// `GitHub` operations that require authentication.
    ///
    /// # Note on zero values
    ///
    /// `GitHub` never issues `app_id` or `installation_id` values of `0`, so this
    /// validation treats `Some(0)` as invalid to catch default/placeholder values
    /// early.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingRequired` if any required field is `None`
    /// or if numeric IDs are zero, with the field names listed in the error
    /// message.
    pub fn validate(&self) -> crate::error::Result<()> {
        let mut missing = Vec::new();
        if self.app_id.is_none() || self.app_id == Some(0) {
            missing.push("github.app_id");
        }
        if self.installation_id.is_none() || self.installation_id == Some(0) {
            missing.push("github.installation_id");
        }
        if self.private_key_path.is_none() {
            missing.push("github.private_key_path");
        }
        if !missing.is_empty() {
            return Err(crate::error::ConfigError::MissingRequired {
                field: missing.join(", "),
            }
            .into());
        }
        Ok(())
    }

    /// Returns whether all `GitHub` credentials are properly configured.
    ///
    /// This checks that all three fields (`app_id`, `installation_id`,
    /// `private_key_path`) are present and that numeric IDs are non-zero.
    /// This mirrors the checks performed by [`validate()`](Self::validate).
    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.app_id.is_some_and(|v| v != 0)
            && self.installation_id.is_some_and(|v| v != 0)
            && self.private_key_path.is_some()
    }
}

/// Sandbox security configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SandboxConfig {
    /// Run the container in privileged mode (less secure but more compatible).
    pub privileged: bool,

    /// Mount /dev/fuse in the container for fuse-overlayfs support.
    pub mount_dev_fuse: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            privileged: false,
            mount_dev_fuse: true,
        }
    }
}

/// Agent execution configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AgentConfig {
    /// The type of agent to run.
    pub kind: AgentKind,

    /// The execution mode for the agent.
    pub mode: AgentMode,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            kind: AgentKind::Claude,
            mode: AgentMode::Podbot,
        }
    }
}

/// Workspace configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct WorkspaceConfig {
    /// Base directory for cloned repositories inside the container.
    pub base_dir: Utf8PathBuf,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            base_dir: Utf8PathBuf::from("/work"),
        }
    }
}

/// Credential copying configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CredsConfig {
    /// Copy `~/.claude` credentials into the container.
    pub copy_claude: bool,

    /// Copy `~/.codex` credentials into the container.
    pub copy_codex: bool,
}

impl Default for CredsConfig {
    fn default() -> Self {
        Self {
            copy_claude: true,
            copy_codex: true,
        }
    }
}

/// Root application configuration.
///
/// This structure is loaded from configuration files, environment variables,
/// and command-line arguments with layered precedence. The precedence order
/// (lowest to highest) is: defaults, configuration file, environment variables,
/// command-line arguments.
///
/// Configuration files are discovered in this order:
/// 1. Path specified via `PODBOT_CONFIG_PATH` environment variable
/// 2. `.podbot.toml` in the current working directory
/// 3. `.podbot.toml` in the home directory
/// 4. `~/.config/podbot/config.toml` (XDG default)
#[derive(Debug, Clone, Default, Deserialize, Serialize, OrthoConfig)]
#[ortho_config(
    prefix = "PODBOT",
    post_merge_hook,
    discovery(
        app_name = "podbot",
        env_var = "PODBOT_CONFIG_PATH",
        config_file_name = "config.toml",
        dotfile_name = ".podbot.toml",
        config_cli_long = "config",
        config_cli_visible = true,
    )
)]
pub struct AppConfig {
    /// The container engine socket path or URL.
    pub engine_socket: Option<String>,

    /// The container image to use for the sandbox.
    pub image: Option<String>,

    /// `GitHub` App configuration.
    #[serde(default)]
    #[ortho_config(skip_cli)]
    pub github: GitHubConfig,

    /// Sandbox security configuration.
    #[serde(default)]
    #[ortho_config(skip_cli)]
    pub sandbox: SandboxConfig,

    /// Agent configuration.
    #[serde(default)]
    #[ortho_config(skip_cli)]
    pub agent: AgentConfig,

    /// Workspace configuration.
    #[serde(default)]
    #[ortho_config(skip_cli)]
    pub workspace: WorkspaceConfig,

    /// Credential copying configuration.
    #[serde(default)]
    #[ortho_config(skip_cli)]
    pub creds: CredsConfig,
}

impl PostMergeHook for AppConfig {
    fn post_merge(&mut self, _ctx: &PostMergeContext) -> OrthoResult<()> {
        // Placeholder for future normalisation logic.
        // GitHub validation is intentionally NOT performed here because
        // not all commands require GitHub credentials (e.g., `podbot ps`).
        Ok(())
    }
}
