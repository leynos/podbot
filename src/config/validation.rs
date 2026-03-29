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
    /// use podbot::config::{AppConfig, CommandIntent, WorkspaceSource};
    ///
    /// let mut config = AppConfig::default();
    /// config.workspace.source = WorkspaceSource::HostMount;
    /// config.workspace.host_path = Some("/tmp/project".into());
    ///
    /// config.normalize_and_validate(CommandIntent::Host)?;
    /// assert_eq!(
    ///     config.workspace.container_path.as_deref(),
    ///     Some("/workspace")
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
        match intent {
            CommandIntent::Any => Ok(()),
            CommandIntent::Run if self.agent.mode == AgentMode::Podbot => Ok(()),
            CommandIntent::Run => invalid_value(
                "agent.mode",
                "hosted modes require `podbot host`; use `agent.mode = \"podbot\"` for `podbot run`",
            ),
            CommandIntent::Host if self.agent.mode != AgentMode::Podbot => Ok(()),
            CommandIntent::Host => invalid_value(
                "agent.mode",
                "interactive mode requires `podbot run`; use `codex_app_server` or `acp` with `podbot host`",
            ),
        }
    }
}

fn validate_env_allowlist(values: &[String]) -> Result<()> {
    for value in values {
        if value.trim().is_empty() {
            return invalid_value(
                "agent.env_allowlist",
                "agent.env_allowlist entries must not be empty",
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
    let host_path = config.workspace.host_path.as_ref().ok_or_else(|| {
        crate::error::PodbotError::from(ConfigError::InvalidValue {
            field: String::from("workspace.host_path"),
            reason: String::from(
                "`workspace.source = \"host_mount\"` requires `workspace.host_path`",
            ),
        })
    })?;

    if !host_path.is_absolute() {
        return invalid_value(
            "workspace.host_path",
            "workspace.host_path must be an absolute host path",
        );
    }

    let container_path = config.workspace.container_path.as_ref().ok_or_else(|| {
        crate::error::PodbotError::from(ConfigError::InvalidValue {
            field: String::from("workspace.container_path"),
            reason: String::from(
                "`workspace.source = \"host_mount\"` requires `workspace.container_path`",
            ),
        })
    })?;

    if !container_path.is_absolute() {
        return invalid_value(
            "workspace.container_path",
            "workspace.container_path must be an absolute container path",
        );
    }

    Ok(())
}

fn invalid_value<T>(field: &str, reason: &str) -> Result<T> {
    Err(ConfigError::InvalidValue {
        field: field.to_owned(),
        reason: reason.to_owned(),
    }
    .into())
}
