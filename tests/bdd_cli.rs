//! Behavioural tests for the podbot CLI.
//!
//! These tests validate the command-line interface behaviour using rstest-bdd.

// Test-specific lint exceptions: expect and unwrap are standard practice in tests
#![expect(clippy::expect_used, reason = "expect is standard practice in tests")]
#![expect(
    clippy::str_to_string,
    reason = "String conversion clarity is more important than micro-optimization in tests"
)]

use clap::{CommandFactory, Parser};
use podbot::config::Cli;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then};

/// State shared across CLI test scenarios.
#[derive(Default, ScenarioState)]
struct CliState {
    /// The output from running the CLI.
    output: Slot<String>,
    /// Any error message from the CLI.
    error: Slot<String>,
    /// Whether the CLI invocation succeeded.
    success: Slot<bool>,
}

/// Fixture providing a fresh CLI state.
#[fixture]
fn cli_state() -> CliState {
    CliState::default()
}

// Step definitions

#[given("the CLI is invoked with --help")]
fn invoke_with_help(cli_state: &CliState) {
    let mut cmd = Cli::command();
    let help_text = cmd.render_help().to_string();
    cli_state.output.set(help_text);
    cli_state.success.set(true);
}

#[given("the CLI is invoked with --version")]
fn invoke_with_version(cli_state: &CliState) {
    let cmd = Cli::command();
    let version = cmd.get_version().unwrap_or("unknown").to_string();
    let name = cmd.get_name();
    cli_state.output.set(format!("{name} {version}"));
    cli_state.success.set(true);
}

#[given("the CLI is invoked with run")]
fn invoke_run_without_args(cli_state: &CliState) {
    // Try to parse "run" without required arguments
    let result: Result<Cli, clap::Error> = Cli::try_parse_from(["podbot", "run"]);
    match result {
        Ok(_) => {
            cli_state.success.set(true);
        }
        Err(e) => {
            cli_state.error.set(e.to_string());
            cli_state.success.set(false);
        }
    }
}

#[given("the CLI is invoked with run --repo owner/name")]
fn invoke_run_with_repo(cli_state: &CliState) {
    // Try to parse "run --repo owner/name" without branch
    let result: Result<Cli, clap::Error> =
        Cli::try_parse_from(["podbot", "run", "--repo", "owner/name"]);
    match result {
        Ok(_) => {
            cli_state.success.set(true);
        }
        Err(e) => {
            cli_state.error.set(e.to_string());
            cli_state.success.set(false);
        }
    }
}

#[then("the output contains {text}")]
fn output_contains(cli_state: &CliState, text: String) {
    let output = cli_state
        .output
        .get()
        .expect("output should be set before checking");
    assert!(
        output.contains(&text),
        "Expected output to contain '{text}', but got:\n{output}"
    );
}

#[then("an error is returned")]
fn error_is_returned(cli_state: &CliState) {
    let success = cli_state
        .success
        .get()
        .expect("success should be set before checking");
    assert!(!success, "Expected an error to be returned");
}

#[then("the error mentions --repo")]
fn error_mentions_repo(cli_state: &CliState) {
    let error = cli_state
        .error
        .get()
        .expect("error should be set before checking");
    assert!(
        error.contains("--repo"),
        "Expected error to mention '--repo', but got:\n{error}"
    );
}

#[then("the error mentions --branch")]
fn error_mentions_branch(cli_state: &CliState) {
    let error = cli_state
        .error
        .get()
        .expect("error should be set before checking");
    assert!(
        error.contains("--branch"),
        "Expected error to mention '--branch', but got:\n{error}"
    );
}

// Scenario bindings

#[scenario(path = "tests/features/cli.feature", name = "Display help information")]
fn display_help_information(cli_state: CliState) {
    let _ = cli_state;
}

#[scenario(
    path = "tests/features/cli.feature",
    name = "Display version information"
)]
fn display_version_information(cli_state: CliState) {
    let _ = cli_state;
}

#[scenario(
    path = "tests/features/cli.feature",
    name = "Run command requires repository"
)]
fn run_requires_repository(cli_state: CliState) {
    let _ = cli_state;
}

#[scenario(
    path = "tests/features/cli.feature",
    name = "Run command requires branch"
)]
fn run_requires_branch(cli_state: CliState) {
    let _ = cli_state;
}
