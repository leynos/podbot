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
use podbot::api::{CommandOutcome, ExecMode, ExecRequest};
use podbot::cli::{Cli, Commands, ExecArgs, HostArgs, StopArgs, TokenDaemonArgs};
use podbot::config::{AppConfig, load_config};
use podbot::error::ConfigError;
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
    let options = cli.config_load_options();
    let config = load_config(&options).map_err(Report::from)?;
    match run(&cli, &config) {
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
fn run(cli: &Cli, config: &AppConfig) -> PodbotResult<CommandOutcome> {
    match &cli.command {
        Commands::Run(args) => {
            let request = args.to_run_request()?;
            run_agent_cli(config, &request)
        }
        Commands::Host(_args) => {
            // TODO: Re-enable `host_agent_cli` once it emits diagnostics to
            // stderr only and cannot corrupt stdout protocol traffic.
            Err(ConfigError::InvalidValue {
                field: String::from("command"),
                reason: String::from(
                    "the host subcommand is temporarily disabled until host_agent_cli writes diagnostics to stderr only",
                ),
            }
            .into())
        }
        Commands::TokenDaemon(args) => run_token_daemon_cli(args),
        Commands::Ps => list_containers_cli(),
        Commands::Stop(args) => stop_container_cli(args),
        Commands::Exec(args) => exec_in_container_cli(config, args),
    }
}

/// CLI adapter for running an AI agent in a sandboxed container.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
fn run_agent_cli(
    config: &AppConfig,
    request: &podbot::api::RunRequest,
) -> PodbotResult<CommandOutcome> {
    println!(
        "Running {:?} agent in {:?} mode for repository {} on branch {}",
        config.agent.kind,
        config.agent.mode,
        request.repository(),
        request.branch()
    );
    if let Some(ref socket) = config.engine_socket {
        println!("Using engine socket: {socket}");
    }
    if let Some(ref image) = config.image {
        println!("Using image: {image}");
    }
    let result = run_agent_api(config, request)?;
    println!("Container orchestration not yet implemented.");
    Ok(result)
}

/// CLI adapter for hosted app-server execution.
#[expect(
    dead_code,
    reason = "temporarily disabled until stdout-safe diagnostics are implemented"
)]
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
fn host_agent_cli(config: &AppConfig, _args: &HostArgs) -> CommandOutcome {
    println!(
        "Hosting {:?} agent in {:?} mode",
        config.agent.kind, config.agent.mode
    );
    println!("Hosted agent orchestration not yet implemented.");
    CommandOutcome::CommandExit { code: 1 }
}

/// CLI adapter for the token refresh daemon.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
fn run_token_daemon_cli(args: &TokenDaemonArgs) -> PodbotResult<CommandOutcome> {
    println!("Starting token daemon for container {}", args.container_id);
    let result = run_token_daemon_api(&args.container_id)?;
    println!("Token daemon not yet implemented.");
    Ok(result)
}

/// CLI adapter for listing running podbot containers.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
fn list_containers_cli() -> PodbotResult<CommandOutcome> {
    println!("Listing podbot containers...");
    let result = list_containers_api()?;
    println!("Container listing not yet implemented.");
    Ok(result)
}

/// CLI adapter for stopping a running container.
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
fn stop_container_cli(args: &StopArgs) -> PodbotResult<CommandOutcome> {
    println!("Stopping container {}", args.container);
    let result = stop_container_api(&args.container)?;
    println!("Container stop not yet implemented.");
    Ok(result)
}

/// CLI adapter for executing a command in a running container.
///
/// Performs terminal detection, builds the library-owned exec request, and
/// delegates engine connection and execution to `podbot::api::exec`.
fn exec_in_container_cli(config: &AppConfig, args: &ExecArgs) -> PodbotResult<CommandOutcome> {
    let mode = if args.detach {
        ExecMode::Detached
    } else {
        ExecMode::Attached
    };
    let tty = !args.detach && std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let request = ExecRequest::new(&args.container, args.command.clone())?
        .with_mode(mode)
        .with_tty(tty);

    podbot::api::exec(config, &request)
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

#[cfg(feature = "experimental")]
fn run_agent_api(
    config: &AppConfig,
    request: &podbot::api::RunRequest,
) -> PodbotResult<CommandOutcome> {
    podbot::api::run_agent(config, request)
}

#[cfg(not(feature = "experimental"))]
fn run_agent_api(
    _config: &AppConfig,
    _request: &podbot::api::RunRequest,
) -> PodbotResult<CommandOutcome> {
    Err(ConfigError::InvalidValue {
        field: String::from("command"),
        reason: String::from("the run command requires feature = \"experimental\""),
    }
    .into())
}

#[cfg(feature = "experimental")]
fn run_token_daemon_api(container_id: &str) -> PodbotResult<CommandOutcome> {
    podbot::api::run_token_daemon(container_id)
}

#[cfg(not(feature = "experimental"))]
fn run_token_daemon_api(_container_id: &str) -> PodbotResult<CommandOutcome> {
    Err(ConfigError::InvalidValue {
        field: String::from("command"),
        reason: String::from("the token-daemon command requires feature = \"experimental\""),
    }
    .into())
}

#[cfg(feature = "experimental")]
fn list_containers_api() -> PodbotResult<CommandOutcome> {
    podbot::api::list_containers()
}

#[cfg(not(feature = "experimental"))]
fn list_containers_api() -> PodbotResult<CommandOutcome> {
    Err(ConfigError::InvalidValue {
        field: String::from("command"),
        reason: String::from("the ps command requires feature = \"experimental\""),
    }
    .into())
}

#[cfg(feature = "experimental")]
fn stop_container_api(container: &str) -> PodbotResult<CommandOutcome> {
    podbot::api::stop_container(container)
}

#[cfg(not(feature = "experimental"))]
fn stop_container_api(_container: &str) -> PodbotResult<CommandOutcome> {
    Err(ConfigError::InvalidValue {
        field: String::from("command"),
        reason: String::from("the stop command requires feature = \"experimental\""),
    }
    .into())
}

#[cfg(test)]
mod tests {
    use podbot::cli::{Cli, Commands, HostArgs};
    use podbot::config::AppConfig;

    use super::{normalize_process_exit_code, run};

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

    #[test]
    fn run_rejects_host_subcommand_until_stdout_is_safe() {
        let cli = Cli {
            command: Commands::Host(HostArgs {
                agent: None,
                mode: None,
            }),
            config: None,
            engine_socket: None,
            image: None,
        };

        let error = run(&cli, &AppConfig::default()).expect_err("host command should be disabled");

        assert!(
            error
                .to_string()
                .contains("host subcommand is temporarily disabled"),
            "unexpected error: {error}",
        );
    }
}
