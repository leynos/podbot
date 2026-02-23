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

use std::io::IsTerminal;

use clap::Parser;
use eyre::{Report, Result as EyreResult};
use podbot::config::{
    AppConfig, Cli, Commands, ExecArgs, RunArgs, StopArgs, TokenDaemonArgs, load_config,
};
use podbot::engine::{EngineConnector, ExecMode, ExecRequest, SocketResolver};
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
        Commands::Run(args) => run_agent(config, args),
        Commands::TokenDaemon(args) => run_token_daemon(args),
        Commands::Ps => list_containers(),
        Commands::Stop(args) => stop_container(args),
        Commands::Exec(args) => exec_in_container(config, args, runtime_handle),
    }
}

enum CommandOutcome {
    Success,
    CommandExit { code: i64 },
}

/// Run an AI agent in a sandboxed container.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub returns Ok; will return errors when container orchestration is implemented"
)]
fn run_agent(config: &AppConfig, args: &RunArgs) -> PodbotResult<CommandOutcome> {
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
    Ok(CommandOutcome::Success)
}

/// Run the token refresh daemon.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub returns Ok; will return errors when token daemon is implemented"
)]
fn run_token_daemon(args: &TokenDaemonArgs) -> PodbotResult<CommandOutcome> {
    println!("Starting token daemon for container {}", args.container_id);
    println!("Token daemon not yet implemented.");
    Ok(CommandOutcome::Success)
}

/// List running podbot containers.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub returns Ok; will return errors when container listing is implemented"
)]
fn list_containers() -> PodbotResult<CommandOutcome> {
    println!("Listing podbot containers...");
    println!("Container listing not yet implemented.");
    Ok(CommandOutcome::Success)
}

/// Stop a running container.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub returns Ok; will return errors when container stop is implemented"
)]
fn stop_container(args: &StopArgs) -> PodbotResult<CommandOutcome> {
    println!("Stopping container {}", args.container);
    println!("Container stop not yet implemented.");
    Ok(CommandOutcome::Success)
}

/// Execute a command in a running container.
fn exec_in_container(
    config: &AppConfig,
    args: &ExecArgs,
    runtime_handle: &tokio::runtime::Handle,
) -> PodbotResult<CommandOutcome> {
    let env = mockable::DefaultEnv::new();
    let resolver = SocketResolver::new(&env);
    let docker =
        EngineConnector::connect_with_fallback(config.engine_socket.as_deref(), &resolver)?;

    let mode = if args.detach {
        ExecMode::Detached
    } else {
        ExecMode::Attached
    };
    let request = ExecRequest::new(&args.container, args.command.clone(), mode)?.with_tty(
        !args.detach && std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
    );

    let exec_result = EngineConnector::exec(runtime_handle, &docker, &request)?;
    if exec_result.exit_code() == 0 {
        Ok(CommandOutcome::Success)
    } else {
        Ok(CommandOutcome::CommandExit {
            code: exec_result.exit_code(),
        })
    }
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
