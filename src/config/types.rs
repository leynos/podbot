//! Root configuration types for podbot.

use camino::Utf8PathBuf;
use ortho_config::{OrthoConfig, OrthoResult, PostMergeContext, PostMergeHook};
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;

use crate::config::{AgentConfig, McpConfig, WorkspaceConfig};

/// How `SELinux` labels should be applied to the container.
///
/// This controls whether the container engine's default `SELinux` labelling
/// is preserved or explicitly disabled. Disabling labels is required for
/// rootless nested `Podman` workflows that fail under strict `SELinux`
/// labelling.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelinuxLabelMode {
    /// Keep engine defaults for `SELinux` labels.
    KeepDefault,

    /// Disable labels for the container process.
    #[default]
    DisableForContainer,
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
    /// This method checks that `app_id`, `installation_id`, and
    /// `private_key_path` are all set and that numeric IDs are non-zero. Call
    /// this before performing `GitHub` operations that require authentication.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingRequired` when any required field is
    /// missing or contains the sentinel value `0`.
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
    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.app_id.is_some_and(|v| v != 0)
            && self.installation_id.is_some_and(|v| v != 0)
            && self.private_key_path.is_some()
    }
}

/// Sandbox security configuration.
#[derive(Debug, Clone, SmartDefault, Deserialize, Serialize)]
#[serde(default)]
pub struct SandboxConfig {
    /// Run the container in privileged mode (less secure but more compatible).
    pub privileged: bool,

    /// Mount /dev/fuse in the container for fuse-overlayfs support.
    #[default = true]
    pub mount_dev_fuse: bool,

    /// `SELinux` label handling mode for the container.
    pub selinux_label_mode: SelinuxLabelMode,
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, OrthoConfig)]
#[ortho_config(prefix = "PODBOT", post_merge_hook)]
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

    /// Defaults for hosted MCP bridge behaviour.
    #[serde(default)]
    #[ortho_config(skip_cli)]
    pub mcp: McpConfig,
}

impl PostMergeHook for AppConfig {
    fn post_merge(&mut self, _ctx: &PostMergeContext) -> OrthoResult<()> {
        Ok(())
    }
}
