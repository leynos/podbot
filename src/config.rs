//! Configuration system for podbot.
//!
//! This module provides the configuration structures and CLI definitions for the
//! podbot application. Configuration loading and precedence merging is handled by
//! the `ortho_config` crate. Intended precedence: CLI flags override environment
//! variables, which override configuration files, which override defaults.
//!
//! The configuration file is expected at `~/.config/podbot/config.toml` by default.
//!
//! # Example Configuration
//!
//! ```toml
//! engine_socket = "unix:///run/user/1000/podman/podman.sock"
//! image = "ghcr.io/example/podbot-sandbox:latest"
//!
//! [github]
//! app_id = 12345
//! installation_id = 67890
//! private_key_path = "/home/user/.config/podbot/github-app.pem"
//!
//! [workspace]
//! base_dir = "/work"
//!
//! [sandbox]
//! privileged = false
//! mount_dev_fuse = true
//!
//! [agent]
//! kind = "claude"
//! ```

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand, ValueEnum};
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

/// GitHub App configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GitHubConfig {
    /// The GitHub App ID.
    pub app_id: Option<u64>,

    /// The GitHub App installation ID.
    pub installation_id: Option<u64>,

    /// Path to the GitHub App private key file.
    pub private_key_path: Option<Utf8PathBuf>,
}

impl GitHubConfig {
    /// Validates that all required GitHub fields are present and non-zero.
    ///
    /// This method checks that `app_id`, `installation_id`, and `private_key_path`
    /// are all set and that numeric IDs are non-zero. Call this before performing
    /// GitHub operations that require authentication.
    ///
    /// # Note on zero values
    ///
    /// GitHub never issues `app_id` or `installation_id` values of `0`, so this
    /// validation treats `Some(0)` as invalid to catch default/placeholder values
    /// early.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingRequired` if any required field is `None` or
    /// if numeric IDs are zero, with the field names listed in the error message.
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

    /// Returns whether all GitHub credentials are properly configured.
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
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            kind: AgentKind::Claude,
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
/// and command-line arguments with layered precedence.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AppConfig {
    /// The container engine socket path or URL.
    pub engine_socket: Option<String>,

    /// The container image to use for the sandbox.
    pub image: Option<String>,

    /// GitHub App configuration.
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
}

/// Command-line interface for podbot.
#[derive(Debug, Parser)]
#[command(name = "podbot")]
#[command(
    author,
    version,
    about = "Sandboxed execution environment for AI coding agents"
)]
pub struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Commands,

    /// Path to configuration file.
    #[arg(long, global = true)]
    pub config: Option<Utf8PathBuf>,

    /// Container engine socket path or URL.
    #[arg(long, global = true)]
    pub engine_socket: Option<String>,

    /// Container image to use.
    #[arg(long, global = true)]
    pub image: Option<String>,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Run an AI agent in a sandboxed container.
    Run(RunArgs),

    /// Run the token refresh daemon.
    TokenDaemon(TokenDaemonArgs),

    /// List running podbot containers.
    Ps,

    /// Stop a running container.
    Stop(StopArgs),

    /// Execute a command in a running container.
    Exec(ExecArgs),
}

/// Arguments for the `run` subcommand.
#[derive(Debug, Parser)]
pub struct RunArgs {
    /// Repository to clone in owner/name format.
    #[arg(long, required = true)]
    pub repo: String,

    /// Branch to check out.
    #[arg(long, required = true)]
    pub branch: String,

    /// Agent type to run.
    #[arg(long, value_enum, default_value_t = AgentKind::Claude)]
    pub agent: AgentKind,
}

/// Arguments for the `token-daemon` subcommand.
#[derive(Debug, Parser)]
pub struct TokenDaemonArgs {
    /// Container ID to manage tokens for.
    #[arg(required = true)]
    pub container_id: String,
}

/// Arguments for the `stop` subcommand.
#[derive(Debug, Parser)]
pub struct StopArgs {
    /// Container ID or name to stop.
    #[arg(required = true)]
    pub container: String,
}

/// Arguments for the `exec` subcommand.
#[derive(Debug, Parser)]
pub struct ExecArgs {
    /// Container ID or name.
    #[arg(required = true)]
    pub container: String,

    /// Command to execute.
    #[arg(required = true, trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::{fixture, rstest};

    /// Fixture providing a default `AppConfig`.
    #[fixture]
    fn app_config() -> AppConfig {
        AppConfig::default()
    }
    /// Fixture providing a default `SandboxConfig`.
    #[fixture]
    fn sandbox_config() -> SandboxConfig {
        SandboxConfig::default()
    }
    /// Fixture providing a default `WorkspaceConfig`.
    #[fixture]
    fn workspace_config() -> WorkspaceConfig {
        WorkspaceConfig::default()
    }
    /// Fixture providing a default `CredsConfig`.
    #[fixture]
    fn creds_config() -> CredsConfig {
        CredsConfig::default()
    }

    /// Fixture providing a fully configured `GitHubConfig`.
    #[fixture]
    fn github_config_complete() -> GitHubConfig {
        GitHubConfig {
            app_id: Some(12345),
            installation_id: Some(67890),
            private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
        }
    }

    #[rstest]
    fn agent_kind_default_is_claude() {
        assert_eq!(AgentKind::default(), AgentKind::Claude);
    }
    #[rstest]
    #[case(AgentKind::Claude, "claude")]
    #[case(AgentKind::Codex, "codex")]
    fn agent_kind_serialises_to_lowercase(#[case] kind: AgentKind, #[case] expected: &str) {
        let serialised = serde_json::to_string(&kind).expect("serialisation should succeed");
        assert_eq!(serialised, format!("\"{expected}\""));
    }

    #[rstest]
    #[case("\"claude\"", AgentKind::Claude)]
    #[case("\"codex\"", AgentKind::Codex)]
    fn agent_kind_deserialises_from_lowercase(#[case] input: &str, #[case] expected: AgentKind) {
        let kind: AgentKind = serde_json::from_str(input).expect("deserialisation should succeed");
        assert_eq!(kind, expected);
    }

    #[rstest]
    fn sandbox_config_default_values(sandbox_config: SandboxConfig) {
        assert!(!sandbox_config.privileged);
        assert!(sandbox_config.mount_dev_fuse);
    }

    #[rstest]
    fn workspace_config_default_base_dir(workspace_config: WorkspaceConfig) {
        assert_eq!(workspace_config.base_dir.as_str(), "/work");
    }

    #[rstest]
    fn creds_config_default_copies_both(creds_config: CredsConfig) {
        assert!(creds_config.copy_claude);
        assert!(creds_config.copy_codex);
    }

    #[rstest]
    fn app_config_has_sensible_defaults(app_config: AppConfig) {
        assert!(app_config.engine_socket.is_none());
        assert!(app_config.image.is_none());
        assert!(app_config.sandbox.mount_dev_fuse);
        assert!(!app_config.sandbox.privileged);
    }

    #[rstest]
    fn app_config_nested_configs_have_defaults(app_config: AppConfig) {
        assert!(app_config.github.app_id.is_none());
        assert!(app_config.github.installation_id.is_none());
        assert!(app_config.github.private_key_path.is_none());
        assert_eq!(app_config.agent.kind, AgentKind::Claude);
        assert_eq!(app_config.workspace.base_dir.as_str(), "/work");
    }

    #[rstest]
    fn app_config_deserialises_from_toml() {
        let toml = r#"
            engine_socket = "unix:///run/podman/podman.sock"
            image = "ghcr.io/example/sandbox:latest"

            [github]
            app_id = 12345
            installation_id = 67890

            [sandbox]
            privileged = true
            mount_dev_fuse = false

            [agent]
            kind = "codex"

            [workspace]
            base_dir = "/home/user/work"
        "#;

        let config: AppConfig = toml::from_str(toml).expect("TOML parsing should succeed");

        assert_eq!(
            config.engine_socket.as_deref(),
            Some("unix:///run/podman/podman.sock")
        );
        assert_eq!(
            config.image.as_deref(),
            Some("ghcr.io/example/sandbox:latest")
        );
        assert_eq!(config.github.app_id, Some(12345));
        assert_eq!(config.github.installation_id, Some(67890));
        assert!(config.sandbox.privileged);
        assert!(!config.sandbox.mount_dev_fuse);
        assert_eq!(config.agent.kind, AgentKind::Codex);
        assert_eq!(config.workspace.base_dir.as_str(), "/home/user/work");
    }

    #[rstest]
    fn app_config_uses_defaults_for_missing_fields() {
        let toml = r#"
            engine_socket = "unix:///tmp/docker.sock"
        "#;

        let config: AppConfig = toml::from_str(toml).expect("TOML parsing should succeed");

        assert_eq!(
            config.engine_socket.as_deref(),
            Some("unix:///tmp/docker.sock")
        );
        // All other fields should have defaults
        assert!(config.image.is_none());
        assert!(config.github.app_id.is_none());
        assert!(!config.sandbox.privileged);
        assert!(config.sandbox.mount_dev_fuse);
        assert_eq!(config.agent.kind, AgentKind::Claude);
        assert_eq!(config.workspace.base_dir.as_str(), "/work");
    }

    #[rstest]
    fn app_config_rejects_invalid_agent_kind() {
        let toml = r#"
            [agent]
            kind = "unknown"
        "#;

        let error = toml::from_str::<AppConfig>(toml)
            .expect_err("TOML parsing should fail for an invalid agent kind");
        assert!(
            error.to_string().contains("unknown variant"),
            "Expected unknown-variant error, got: {error}"
        );
    }

    // GitHubConfig validation tests

    #[rstest]
    fn github_config_validate_succeeds_when_complete(github_config_complete: GitHubConfig) {
        let result = github_config_complete.validate();
        assert!(
            result.is_ok(),
            "Expected validation to succeed for complete config"
        );
    }

    #[rstest]
    fn github_config_validate_fails_when_app_id_missing() {
        let config = GitHubConfig {
            app_id: None,
            installation_id: Some(67890),
            private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
        };
        let result = config.validate();
        let error = result.expect_err("validation should fail");
        match error {
            crate::error::PodbotError::Config(crate::error::ConfigError::MissingRequired {
                field,
            }) => {
                assert!(
                    field.contains("github.app_id"),
                    "Field should contain 'github.app_id', got: {field}"
                );
            }
            other => panic!("Expected ConfigError::MissingRequired, got: {other:?}"),
        }
    }

    #[rstest]
    #[case(
        None,
        None,
        None,
        "github.app_id, github.installation_id, github.private_key_path"
    )]
    #[case(
        Some(123),
        None,
        None,
        "github.installation_id, github.private_key_path"
    )]
    #[case(None, Some(456), None, "github.app_id, github.private_key_path")]
    #[case(
        None,
        None,
        Some(Utf8PathBuf::from("/k.pem")),
        "github.app_id, github.installation_id"
    )]
    #[case(Some(123), Some(456), None, "github.private_key_path")]
    #[case(
        Some(123),
        None,
        Some(Utf8PathBuf::from("/k.pem")),
        "github.installation_id"
    )]
    #[case(None, Some(456), Some(Utf8PathBuf::from("/k.pem")), "github.app_id")]
    fn github_config_validate_reports_missing_fields(
        #[case] app_id: Option<u64>,
        #[case] installation_id: Option<u64>,
        #[case] private_key_path: Option<Utf8PathBuf>,
        #[case] expected_fields: &str,
    ) {
        let config = GitHubConfig {
            app_id,
            installation_id,
            private_key_path,
        };
        let result = config.validate();
        let error = result.expect_err("validation should fail with missing fields");
        match error {
            crate::error::PodbotError::Config(crate::error::ConfigError::MissingRequired {
                field,
            }) => {
                assert_eq!(
                    field, expected_fields,
                    "Field mismatch: expected '{expected_fields}', got '{field}'"
                );
            }
            other => panic!("Expected ConfigError::MissingRequired, got: {other:?}"),
        }
    }

    #[rstest]
    fn github_config_is_configured_true_when_complete(github_config_complete: GitHubConfig) {
        assert!(github_config_complete.is_configured());
    }

    #[rstest]
    fn github_config_is_configured_false_when_default() {
        let config = GitHubConfig::default();
        assert!(!config.is_configured());
    }

    #[rstest]
    fn github_config_is_configured_false_when_partial() {
        let config = GitHubConfig {
            app_id: Some(12345),
            installation_id: None,
            private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
        };
        assert!(!config.is_configured());
    }

    #[rstest]
    fn github_config_is_configured_false_when_app_id_is_zero() {
        let config = GitHubConfig {
            app_id: Some(0),
            installation_id: Some(67890),
            private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
        };
        assert!(!config.is_configured());
    }

    #[rstest]
    fn github_config_is_configured_false_when_installation_id_is_zero() {
        let config = GitHubConfig {
            app_id: Some(12345),
            installation_id: Some(0),
            private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
        };
        assert!(!config.is_configured());
    }

    #[rstest]
    fn github_config_validate_fails_when_app_id_is_zero() {
        let config = GitHubConfig {
            app_id: Some(0),
            installation_id: Some(67890),
            private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
        };
        let result = config.validate();
        let error = result.expect_err("validation should fail for zero app_id");
        match error {
            crate::error::PodbotError::Config(crate::error::ConfigError::MissingRequired {
                field,
            }) => {
                assert!(
                    field.contains("github.app_id"),
                    "Field should contain 'github.app_id', got: {field}"
                );
            }
            other => panic!("Expected ConfigError::MissingRequired, got: {other:?}"),
        }
    }

    #[rstest]
    fn github_config_validate_fails_when_installation_id_is_zero() {
        let config = GitHubConfig {
            app_id: Some(12345),
            installation_id: Some(0),
            private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
        };
        let result = config.validate();
        let error = result.expect_err("validation should fail for zero installation_id");
        match error {
            crate::error::PodbotError::Config(crate::error::ConfigError::MissingRequired {
                field,
            }) => {
                assert!(
                    field.contains("github.installation_id"),
                    "Field should contain 'github.installation_id', got: {field}"
                );
            }
            other => panic!("Expected ConfigError::MissingRequired, got: {other:?}"),
        }
    }

    #[rstest]
    fn github_config_validate_fails_when_both_ids_are_zero() {
        let config = GitHubConfig {
            app_id: Some(0),
            installation_id: Some(0),
            private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
        };
        let result = config.validate();
        let error = result.expect_err("validation should fail for zero IDs");
        match error {
            crate::error::PodbotError::Config(crate::error::ConfigError::MissingRequired {
                field,
            }) => {
                assert_eq!(
                    field, "github.app_id, github.installation_id",
                    "Field mismatch: got '{field}'"
                );
            }
            other => panic!("Expected ConfigError::MissingRequired, got: {other:?}"),
        }
    }
}
