//! Semantic configuration normalization and legality checks.

use crate::config::{
    AgentKind, AgentMode, AppConfig, WorkspaceSource, default_host_mount_container_path,
};
use crate::error::{ConfigError, Result};

/// High-level command intent used for configuration legality checks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CommandIntent {
    /// Validate only schema-internal invariants.
    #[default]
    Any,
    /// Validate the config as an interactive `podbot run` request.
    Run,
    /// Validate the config as a hosted `podbot host` request.
    Host,
}

impl AppConfig {
    /// Normalize dependent defaults and validate semantic configuration rules.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use camino::Utf8Path;
    /// use podbot::config::{AgentMode, AppConfig, CommandIntent, WorkspaceSource};
    ///
    /// let mut config = AppConfig::default();
    /// config.agent.mode = AgentMode::CodexAppServer;
    /// config.workspace.source = WorkspaceSource::HostMount;
    /// config.workspace.host_path = Some("/tmp/project".into());
    ///
    /// config.normalize_and_validate(CommandIntent::Host)?;
    /// assert_eq!(
    ///     config.workspace.container_path.as_deref(),
    ///     Some(Utf8Path::new("/workspace"))
    /// );
    /// # Ok::<(), podbot::error::PodbotError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::InvalidValue` when semantic config invariants are
    /// violated, such as illegal `(command, agent.mode)` combinations or
    /// missing `host_mount` paths.
    pub fn normalize_and_validate(&mut self, intent: CommandIntent) -> Result<()> {
        self.apply_dependent_defaults();
        self.validate_agent_config()?;
        self.validate_workspace_config()?;
        self.validate_command_intent(intent)
    }

    fn apply_dependent_defaults(&mut self) {
        if self.workspace.source == WorkspaceSource::HostMount
            && self.workspace.container_path.is_none()
        {
            self.workspace.container_path = Some(default_host_mount_container_path());
        }
    }

    fn validate_agent_config(&self) -> Result<()> {
        validate_env_allowlist(&self.agent.env_allowlist)?;

        match self.agent.kind {
            AgentKind::Custom => validate_custom_agent(self),
            AgentKind::Claude | AgentKind::Codex => validate_builtin_agent(self),
        }
    }

    fn validate_workspace_config(&self) -> Result<()> {
        if !self.workspace.base_dir.is_absolute() {
            return invalid_value(
                "workspace.base_dir",
                "workspace.base_dir must be an absolute container path",
            );
        }

        match self.workspace.source {
            WorkspaceSource::GithubClone => validate_github_clone_workspace(self),
            WorkspaceSource::HostMount => validate_host_mount_workspace(self),
        }
    }

    fn validate_command_intent(&self, intent: CommandIntent) -> Result<()> {
        if is_intent_legal(intent, self.agent.mode) {
            Ok(())
        } else {
            match intent {
                CommandIntent::Run => invalid_value(
                    "agent.mode",
                    format!(
                        "hosted modes require `podbot host`; use `agent.mode = \"podbot\"` for `podbot run` (current mode: {})",
                        self.agent.mode.as_token()
                    ),
                ),
                CommandIntent::Host => invalid_value(
                    "agent.mode",
                    format!(
                        "interactive mode requires `podbot run`; use `codex_app_server` or `acp` with `podbot host` (current mode: {})",
                        self.agent.mode.as_token()
                    ),
                ),
                CommandIntent::Any => Ok(()), // unreachable: Any is always legal
            }
        }
    }
}

fn is_intent_legal(intent: CommandIntent, mode: AgentMode) -> bool {
    match intent {
        CommandIntent::Any => true,
        CommandIntent::Run => mode == AgentMode::Podbot,
        CommandIntent::Host => matches!(mode, AgentMode::CodexAppServer | AgentMode::Acp),
    }
}

fn validate_env_allowlist(values: &[String]) -> Result<()> {
    for value in values {
        if value.trim().is_empty() {
            return invalid_value(
                "agent.env_allowlist",
                "agent.env_allowlist entries must not be empty or whitespace only",
            );
        }
    }

    Ok(())
}

fn validate_custom_agent(config: &AppConfig) -> Result<()> {
    match config.agent.command.as_deref().map(str::trim) {
        Some(command) if !command.is_empty() => Ok(()),
        _ => invalid_value(
            "agent.command",
            "`agent.kind = \"custom\"` requires a non-empty `agent.command`",
        ),
    }
}

fn validate_builtin_agent(config: &AppConfig) -> Result<()> {
    if config.agent.command.is_some() {
        return invalid_value(
            "agent.command",
            "built-in agent kinds must not set `agent.command`; use `agent.kind = \"custom\"` instead",
        );
    }

    if !config.agent.args.is_empty() {
        return invalid_value(
            "agent.args",
            "built-in agent kinds must not set `agent.args`; use `agent.kind = \"custom\"` instead",
        );
    }

    Ok(())
}

fn validate_github_clone_workspace(config: &AppConfig) -> Result<()> {
    if config.workspace.host_path.is_some() {
        return invalid_value(
            "workspace.host_path",
            "`workspace.host_path` is only valid when `workspace.source = \"host_mount\"`",
        );
    }

    if config.workspace.container_path.is_some() {
        return invalid_value(
            "workspace.container_path",
            "`workspace.container_path` is only valid when `workspace.source = \"host_mount\"`",
        );
    }

    Ok(())
}

fn validate_host_mount_workspace(config: &AppConfig) -> Result<()> {
    let Some(host_path) = config.workspace.host_path.as_ref() else {
        return invalid_value(
            "workspace.host_path",
            "`workspace.source = \"host_mount\"` requires `workspace.host_path`",
        );
    };

    if !host_path.is_absolute() {
        return invalid_value(
            "workspace.host_path",
            "workspace.host_path must be an absolute host path",
        );
    }

    // Validate container_path absoluteness; apply_dependent_defaults ensures it's set
    let is_absolute = config.workspace.container_path.as_ref().map_or_else(
        || default_host_mount_container_path().is_absolute(),
        |p| p.is_absolute(),
    );

    if !is_absolute {
        return invalid_value(
            "workspace.container_path",
            "workspace.container_path must be an absolute container path",
        );
    }

    Ok(())
}

fn invalid_value<T>(field: &str, reason: impl Into<String>) -> Result<T> {
    Err(ConfigError::InvalidValue {
        field: field.to_owned(),
        reason: reason.into(),
    }
    .into())
}
