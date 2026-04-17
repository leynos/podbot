//! Root configuration types for podbot.

use std::{borrow::Cow, sync::Arc};

use camino::Utf8PathBuf;
use ortho_config::declarative::{from_value_merge, merge_value};
use ortho_config::{MergeLayer, MergeProvenance, OrthoResult, PostMergeContext, PostMergeHook};
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

    /// Returns `true` if any `GitHub` credential field has been set.
    ///
    /// Use this to decide whether complete credential configuration should be
    /// enforced: if any field is present, call [`Self::validate`] to confirm
    /// all required fields are also present.
    #[must_use]
    pub const fn is_partially_configured(&self) -> bool {
        self.app_id.is_some() || self.installation_id.is_some() || self.private_key_path.is_some()
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
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AppConfig {
    /// The container engine socket path or URL.
    pub engine_socket: Option<String>,

    /// The container image to use for the sandbox.
    pub image: Option<String>,

    /// `GitHub` App configuration.
    #[serde(default)]
    pub github: GitHubConfig,

    /// Sandbox security configuration.
    #[serde(default)]
    pub sandbox: SandboxConfig,

    /// Agent configuration.
    #[serde(default)]
    pub agent: AgentConfig,

    /// Workspace configuration.
    #[serde(default)]
    pub workspace: WorkspaceConfig,

    /// Credential copying configuration.
    #[serde(default)]
    pub creds: CredsConfig,

    /// Defaults for hosted MCP bridge behaviour.
    #[serde(default)]
    pub mcp: McpConfig,
}

impl PostMergeHook for AppConfig {
    fn post_merge(&mut self, _ctx: &PostMergeContext) -> OrthoResult<()> {
        Ok(())
    }
}

impl AppConfig {
    /// Merge application configuration from declarative layers.
    ///
    /// This mirrors the `ortho_config` derive-generated `merge_from_layers`
    /// constructor without pulling `clap` into non-CLI library builds.
    ///
    /// # Errors
    ///
    /// Returns an `OrthoError` when the accumulated JSON layers cannot be
    /// deserialized into `Self` or when the post-merge hook fails.
    pub fn merge_from_layers<'a, I>(layers: I) -> ortho_config::OrthoResult<Self>
    where
        I: IntoIterator<Item = MergeLayer<'a>>,
    {
        let mut merged =
            ortho_config::serde_json::Value::Object(ortho_config::serde_json::Map::new());
        let mut ctx = PostMergeContext::new(Self::prefix());

        let defaults_layer = serialized_defaults_layer()?;
        ensure_defaults_layer_is_not_empty(&defaults_layer)?;
        merge_value(&mut merged, defaults_layer.into_value());

        for layer in layers {
            if layer.provenance() == MergeProvenance::Defaults {
                ensure_defaults_layer_is_not_empty(&layer)?;
            }
            if let Some(path) = layer.path() {
                ctx.with_file(path.to_owned());
            }
            if layer.provenance() == MergeProvenance::Cli {
                ctx.with_cli_input();
            }
            merge_value(&mut merged, layer.into_value());
        }

        let mut result = from_value_merge(merged)?;
        Self::post_merge(&mut result, &ctx)?;
        Ok(result)
    }

    /// Prefix used for `PODBOT_*` environment variables.
    #[must_use]
    pub const fn prefix() -> &'static str {
        "PODBOT"
    }
}

fn ensure_defaults_layer_is_not_empty(layer: &MergeLayer<'_>) -> ortho_config::OrthoResult<()> {
    let defaults_value = layer.clone().into_value();
    let is_empty_object = matches!(
        defaults_value,
        ortho_config::serde_json::Value::Object(ref fields) if fields.is_empty()
    );
    if is_empty_object {
        return Err(std::sync::Arc::new(ortho_config::OrthoError::Validation {
            key: String::from("defaults"),
            message: String::from(
                "merge_from_layers requires a serialized AppConfig::default() layer",
            ),
        }));
    }

    Ok(())
}

fn serialized_defaults_layer() -> ortho_config::OrthoResult<MergeLayer<'static>> {
    let value = ortho_config::serde_json::to_value(AppConfig::default()).map_err(|error| {
        Arc::new(ortho_config::OrthoError::Validation {
            key: String::from("defaults"),
            message: format!("failed to serialize AppConfig::default(): {error}"),
        })
    })?;

    Ok(MergeLayer::defaults(Cow::Owned(value)))
}
