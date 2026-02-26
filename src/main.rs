//! `podbot` application entry point.
//!
//! This binary is a thin CLI adapter over the `podbot` library. It handles
//! argument parsing via Clap, operator-facing output formatting, and process
//! exit code conversion. All business logic lives in `podbot::api`.
//!
//! Configuration is loaded with layered precedence via `OrthoConfig`:
//! 1. Application defaults
//! 2. Configuration file (`~/.config/podbot/config.toml` or path from `PODBOT_CONFIG_PATH`)
//! 3. Environment variables (`PODBOT_*`)
//! 4. Command-line arguments

use std::io::IsTerminal;

use clap::Parser;
use eyre::{Report, Result as EyreResult};
use podbot::api::{CommandOutcome, ExecParams};
use podbot::config::{
    AppConfig, Cli, Commands, ExecArgs, RunArgs, StopArgs, TokenDaemonArgs, load_config,
};
use podbot::engine::ExecMode;
use podbot::error::{ContainerError, Result as PodbotResult};

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
    let runtime = create_runtime().map_err(Report::from)?;

    match run(&cli, &config, runtime.handle()) {
        Ok(CommandOutcome::Success) => Ok(()),
        Ok(CommandOutcome::CommandExit { code }) => {
            std::process::exit(normalize_process_exit_code(code))
        }
        Err(error) => Err(Report::from(error)),
    }
}

/// Execute the CLI command, returning domain-specific errors.
///
/// Keeps semantic errors inside the run loop so the CLI boundary owns
/// conversion to `eyre::Report`.
fn run(
    cli: &Cli,
    config: &AppConfig,
    runtime_handle: &tokio::runtime::Handle,
) -> PodbotResult<CommandOutcome> {
    match &cli.command {
        Commands::Run(args) => run_agent_cli(config, args),
        Commands::TokenDaemon(args) => run_token_daemon_cli(args),
        Commands::Ps => list_containers_cli(),
        Commands::Stop(args) => stop_container_cli(args),
        Commands::Exec(args) => exec_in_container_cli(config, args, runtime_handle),
    }
}

/// CLI adapter for running an AI agent in a sandboxed container.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
fn run_agent_cli(config: &AppConfig, args: &RunArgs) -> PodbotResult<CommandOutcome> {
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
    let result = podbot::api::run_agent(config)?;
    println!("Container orchestration not yet implemented.");
    Ok(result)
}

/// CLI adapter for the token refresh daemon.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
fn run_token_daemon_cli(args: &TokenDaemonArgs) -> PodbotResult<CommandOutcome> {
    println!("Starting token daemon for container {}", args.container_id);
    let result = podbot::api::run_token_daemon(&args.container_id)?;
    println!("Token daemon not yet implemented.");
    Ok(result)
}

/// CLI adapter for listing running podbot containers.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
fn list_containers_cli() -> PodbotResult<CommandOutcome> {
    println!("Listing podbot containers...");
    let result = podbot::api::list_containers()?;
    println!("Container listing not yet implemented.");
    Ok(result)
}

/// CLI adapter for stopping a running container.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
fn stop_container_cli(args: &StopArgs) -> PodbotResult<CommandOutcome> {
    println!("Stopping container {}", args.container);
    let result = podbot::api::stop_container(&args.container)?;
    println!("Container stop not yet implemented.");
    Ok(result)
}

/// CLI adapter for executing a command in a running container.
///
/// Performs terminal detection (a CLI concern) before delegating to the
/// library orchestration function.
fn exec_in_container_cli(
    config: &AppConfig,
    args: &ExecArgs,
    runtime_handle: &tokio::runtime::Handle,
) -> PodbotResult<CommandOutcome> {
    let mode = if args.detach {
        ExecMode::Detached
    } else {
        ExecMode::Attached
    };
    let tty = !args.detach && std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let env = mockable::DefaultEnv::new();

    podbot::api::exec(ExecParams {
        config,
        container: &args.container,
        command: args.command.clone(),
        mode,
        tty,
        runtime_handle,
        env: &env,
    })
}

fn create_runtime() -> PodbotResult<tokio::runtime::Runtime> {
    tokio::runtime::Runtime::new().map_err(|error| {
        podbot::error::PodbotError::from(ContainerError::RuntimeCreationFailed {
            message: error.to_string(),
        })
    })
}

/// Normalize container exit codes to process exit codes.
///
/// Container engines can report values outside the platform shell convention.
/// podbot preserves values in the `0..=255` range, maps negative values to `1`,
/// and clamps oversized values to `255`.
fn normalize_process_exit_code(code: i64) -> i32 {
    const MAX_PROCESS_EXIT_CODE: i64 = u8::MAX as i64;

    if code < 0 {
        return 1;
    }
    if code > MAX_PROCESS_EXIT_CODE {
        return i32::from(u8::MAX);
    }

    i32::try_from(code).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::normalize_process_exit_code;

    #[test]
    fn normalize_process_exit_code_preserves_valid_range() {
        assert_eq!(normalize_process_exit_code(0), 0);
        assert_eq!(normalize_process_exit_code(42), 42);
        assert_eq!(normalize_process_exit_code(255), 255);
    }

    #[test]
    fn normalize_process_exit_code_maps_negative_values_to_one() {
        assert_eq!(normalize_process_exit_code(-1), 1);
        assert_eq!(normalize_process_exit_code(i64::MIN), 1);
    }

    #[test]
    fn normalize_process_exit_code_clamps_oversized_values() {
        assert_eq!(normalize_process_exit_code(256), 255);
        assert_eq!(normalize_process_exit_code(i64::MAX), 255);
    }
}
