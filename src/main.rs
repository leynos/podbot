//! `podbot` application entry point.
//!
//! This binary provides a sandboxed execution environment for AI coding agents.
//! It uses `eyre` for opaque error handling at the application boundary, converting
//! domain-specific errors into human-readable reports.

use clap::Parser;
use eyre::Result;
use podbot::config::{Cli, Commands};

/// Application entry point.
///
/// Uses `eyre::Result` as the return type to provide human-readable error reports
/// with backtraces when available.
fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Run(args) => run_agent(&cli, args),
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
    reason = "Stub: will return errors when implementation is complete"
)]
fn run_agent(_cli: &Cli, args: &podbot::config::RunArgs) -> Result<()> {
    println!(
        "Running {:?} agent for repository {} on branch {}",
        args.agent, args.repo, args.branch
    );
    println!("Container orchestration not yet implemented.");
    Ok(())
}

/// Run the token refresh daemon.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "Stub: will return errors when implementation is complete"
)]
fn run_token_daemon(args: &podbot::config::TokenDaemonArgs) -> Result<()> {
    println!("Starting token daemon for container {}", args.container_id);
    println!("Token daemon not yet implemented.");
    Ok(())
}

/// List running podbot containers.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "Stub: will return errors when implementation is complete"
)]
fn list_containers() -> Result<()> {
    println!("Listing podbot containers...");
    println!("Container listing not yet implemented.");
    Ok(())
}

/// Stop a running container.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "Stub: will return errors when implementation is complete"
)]
fn stop_container(args: &podbot::config::StopArgs) -> Result<()> {
    println!("Stopping container {}", args.container);
    println!("Container stop not yet implemented.");
    Ok(())
}

/// Execute a command in a running container.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "Stub: will return errors when implementation is complete"
)]
fn exec_in_container(args: &podbot::config::ExecArgs) -> Result<()> {
    println!(
        "Executing command in container {}: {:?}",
        args.container, args.command
    );
    println!("Container exec not yet implemented.");
    Ok(())
}
