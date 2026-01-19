//! Command-line argument definitions for podbot.
//!
//! This module defines the command-line interface for podbot, including global
//! configuration flags and subcommands.

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

use super::{AgentKind, AgentMode};

/// Command-line interface for podbot.
///
/// Configuration is loaded with layered precedence:
/// 1. Application defaults
/// 2. Configuration file (discovered via XDG paths or `PODBOT_CONFIG_PATH`)
/// 3. Environment variables (`PODBOT_*`)
/// 4. Command-line arguments (these flags)
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
    /// Overrides the default discovery paths. Can also be set via `PODBOT_CONFIG_PATH`.
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

    /// Agent execution mode.
    #[arg(long = "agent-mode", value_enum, default_value_t = AgentMode::Podbot)]
    pub mode: AgentMode,
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
