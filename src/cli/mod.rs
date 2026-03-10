//! Command-line interface (CLI) adapter types.
//!
//! This module contains Clap-dependent parse structures used by the `podbot`
//! binary. It is intentionally separate from `podbot::config` so library
//! embedders can load configuration without constructing CLI parse types.

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand, ValueEnum};

use crate::config::{AgentKind, AgentMode, ConfigLoadOptions, ConfigOverrides};

/// CLI-facing agent kind values.
///
/// This type exists to keep Clap-specific derives out of the library
/// configuration model types.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum AgentKindArg {
    /// Claude Code agent.
    #[default]
    Claude,
    /// `OpenAI` Codex agent.
    Codex,
}

impl From<AgentKindArg> for AgentKind {
    fn from(value: AgentKindArg) -> Self {
        match value {
            AgentKindArg::Claude => Self::Claude,
            AgentKindArg::Codex => Self::Codex,
        }
    }
}

/// CLI-facing agent execution mode values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum AgentModeArg {
    /// Run the agent in podbot-managed mode.
    #[default]
    Podbot,
}

impl From<AgentModeArg> for AgentMode {
    fn from(value: AgentModeArg) -> Self {
        match value {
            AgentModeArg::Podbot => Self::Podbot,
        }
    }
}

/// Command-line interface for podbot.
///
/// Configuration is loaded with layered precedence:
///
/// 1. Application defaults
/// 2. Configuration file (discovered via XDG paths or `PODBOT_CONFIG_PATH`)
/// 3. Environment variables (`PODBOT_*`)
/// 4. Command-line overrides (global flags on this struct)
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
    ///
    /// Overrides the default discovery paths. Can also be set via
    /// `PODBOT_CONFIG_PATH`.
    #[arg(short = 'c', long, global = true)]
    pub config: Option<Utf8PathBuf>,

    /// Container engine socket path or URL.
    ///
    /// Can also be set via `PODBOT_ENGINE_SOCKET` or in the configuration file.
    #[arg(long, global = true)]
    pub engine_socket: Option<String>,

    /// Container image to use for the sandbox.
    ///
    /// Can also be set via `PODBOT_IMAGE` or in the configuration file.
    #[arg(long, global = true)]
    pub image: Option<String>,
}

impl Cli {
    /// Convert parsed CLI flags into library load options.
    #[must_use]
    pub fn config_load_options(&self) -> ConfigLoadOptions {
        ConfigLoadOptions {
            config_path_hint: self.config.clone(),
            discover_config: true,
            overrides: ConfigOverrides {
                engine_socket: self.engine_socket.clone(),
                image: self.image.clone(),
            },
        }
    }
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
    #[arg(long, value_enum, default_value_t = AgentKindArg::Claude)]
    pub agent: AgentKindArg,

    /// Agent execution mode.
    #[arg(long = "agent-mode", value_enum, default_value_t = AgentModeArg::Podbot)]
    pub mode: AgentModeArg,
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

    /// Run the command in detached mode (no stream attachment).
    #[arg(short = 'd', long, default_value_t = false)]
    pub detach: bool,

    /// Command to execute.
    #[arg(required = true, trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[cfg(test)]
mod tests;
