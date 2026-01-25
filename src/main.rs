//! `podbot` application entry point.
//!
//! This binary provides a sandboxed execution environment for AI coding agents.
//! It uses `eyre` for opaque error handling at the application boundary, converting
//! domain-specific errors into human-readable reports.
//!
//! Configuration is loaded with layered precedence via `OrthoConfig`:
//! 1. Application defaults
//! 2. Configuration file (`~/.config/podbot/config.toml` or path from `PODBOT_CONFIG_PATH`)
//! 3. Environment variables (`PODBOT_*`)
//! 4. Command-line arguments

use clap::Parser;
use eyre::{Report, Result as EyreResult};
use podbot::config::{
    AppConfig, Cli, Commands, ExecArgs, RunArgs, StopArgs, TokenDaemonArgs, load_config,
};
use podbot::error::Result as PodbotResult;

/// Application entry point.
///
/// Loads configuration with layered precedence via `OrthoConfig`, then dispatches
/// to the appropriate subcommand handler.
///
/// Uses `eyre::Result` as the return type to provide human-readable error reports
/// with backtraces when available.
fn main() -> EyreResult<()> {
    // Parse CLI first (for subcommand dispatch and global options).
    let cli = Cli::parse();

    // Load configuration with layered precedence: defaults < file < env < CLI.
    // The CLI is passed to extract --config, --engine-socket, and --image.
    let config = load_config(&cli).map_err(Report::from)?;

    run(&cli, &config).map_err(Report::from)
}

/// Execute the CLI command, returning domain-specific errors.
///
/// Keeps semantic errors inside the run loop so the CLI boundary owns
/// conversion to `eyre::Report`.
fn run(cli: &Cli, config: &AppConfig) -> PodbotResult<()> {
    match &cli.command {
        Commands::Run(args) => run_agent(config, args),
        Commands::TokenDaemon(args) => run_token_daemon(args),
        Commands::Ps => list_containers(),
        Commands::Stop(args) => stop_container(args),
        Commands::Exec(args) => exec_in_container(args),
    }
}

/// Run an AI agent in a sandboxed container.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub returns Ok; will return errors when container orchestration is implemented"
)]
fn run_agent(config: &AppConfig, args: &RunArgs) -> PodbotResult<()> {
    println!(
        "Running {:?} agent for repository {} on branch {}",
        args.agent, args.repo, args.branch
    );
    if let Some(ref socket) = config.engine_socket {
        println!("Using engine socket: {socket}");
    }
    if let Some(ref image) = config.image {
        println!("Using image: {image}");
    }
    println!("Container orchestration not yet implemented.");
    Ok(())
}

/// Run the token refresh daemon.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub returns Ok; will return errors when token daemon is implemented"
)]
fn run_token_daemon(args: &TokenDaemonArgs) -> PodbotResult<()> {
    println!("Starting token daemon for container {}", args.container_id);
    println!("Token daemon not yet implemented.");
    Ok(())
}

/// List running podbot containers.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub returns Ok; will return errors when container listing is implemented"
)]
fn list_containers() -> PodbotResult<()> {
    println!("Listing podbot containers...");
    println!("Container listing not yet implemented.");
    Ok(())
}

/// Stop a running container.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub returns Ok; will return errors when container stop is implemented"
)]
fn stop_container(args: &StopArgs) -> PodbotResult<()> {
    println!("Stopping container {}", args.container);
    println!("Container stop not yet implemented.");
    Ok(())
}

/// Execute a command in a running container.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub returns Ok; will return errors when container exec is implemented"
)]
fn exec_in_container(args: &ExecArgs) -> PodbotResult<()> {
    println!(
        "Executing command in container {}: {:?}",
        args.container, args.command
    );
    println!("Container exec not yet implemented.");
    Ok(())
}
