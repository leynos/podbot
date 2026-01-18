//! Command-line argument definitions for podbot.

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

use super::AgentKind;

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
