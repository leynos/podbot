//! Behavioural tests for command orchestration.

#![cfg(feature = "internal")]

mod bdd_orchestration_helpers;
mod test_utils;

use bdd_orchestration_helpers::{OrchestrationState, orchestration_state};
use rstest_bdd_macros::scenario;
use serial_test::serial;

#[scenario(
    path = "tests/features/orchestration.feature",
    name = "Exec orchestration returns success for zero exit code"
)]
#[serial]
fn exec_orchestration_returns_success(orchestration_state: OrchestrationState) {
    let _ = orchestration_state;
}

#[scenario(
    path = "tests/features/orchestration.feature",
    name = "Exec orchestration returns command exit for non-zero exit code"
)]
#[serial]
fn exec_orchestration_returns_command_exit(orchestration_state: OrchestrationState) {
    let _ = orchestration_state;
}

#[scenario(
    path = "tests/features/orchestration.feature",
    name = "Run orchestration rejects incomplete GitHub configuration"
)]
#[cfg(feature = "experimental")]
#[serial]
fn run_rejects_incomplete_github_configuration(orchestration_state: OrchestrationState) {
    let _ = orchestration_state;
}

#[scenario(
    path = "tests/features/orchestration.feature",
    name = "Stop orchestration stops a managed container"
)]
#[serial]
fn stop_stops_managed_container(orchestration_state: OrchestrationState) {
    let _ = orchestration_state;
}

#[scenario(
    path = "tests/features/orchestration.feature",
    name = "Stop orchestration refuses an unmanaged container"
)]
#[serial]
fn stop_refuses_unmanaged_container(orchestration_state: OrchestrationState) {
    let _ = orchestration_state;
}

#[scenario(
    path = "tests/features/orchestration.feature",
    name = "List containers orchestration returns managed containers"
)]
#[serial]
fn list_returns_managed_containers(orchestration_state: OrchestrationState) {
    let _ = orchestration_state;
}

#[scenario(
    path = "tests/features/orchestration.feature",
    name = "Token daemon rejects a blank container identifier"
)]
#[cfg(feature = "experimental")]
#[serial]
fn token_daemon_rejects_blank_container(orchestration_state: OrchestrationState) {
    let _ = orchestration_state;
}
