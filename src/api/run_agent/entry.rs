//! Production entry point for `run_agent`.
//!
//! Resolves the container engine, GitHub App client, clock, environment, and
//! host command runner, then delegates the lifecycle to [`AgentRunner`].
//! Keeping the wiring here leaves the lifecycle code focused on orchestration
//! and keeps validation ahead of every engine or network side effect.

use mockable::{DefaultClock, DefaultEnv};

use crate::api::{CommandOutcome, RunRequest};
use crate::config::AppConfig;
use crate::engine::SystemCommandRunner;
use crate::error::Result as PodbotResult;
use crate::github::{OctocrabAppClient, build_app_client, load_private_key};

use super::{AgentRunner, request::needs_github_token};

/// Run an AI agent in a sandboxed container.
///
/// This is the production entry point. It connects to the container engine,
/// builds the GitHub client when the workspace is cloned, and delegates the
/// lifecycle to [`AgentRunner`].
///
/// # Errors
///
/// Returns configuration errors for invalid requests or incomplete GitHub
/// credentials, and container, filesystem, GitHub, or exec errors from the
/// lifecycle.
pub fn run_agent(config: &AppConfig, request: &RunRequest) -> PodbotResult<CommandOutcome> {
    validate_run_inputs(config, request)?;
    let runtime = crate::api::create_runtime()?;
    let docker = crate::api::connect_engine(config)?;
    let env = DefaultEnv::new();
    let clock = DefaultClock;
    let host_runner = SystemCommandRunner;
    let token_client = build_token_client(config, &runtime)?;

    let runner = AgentRunner {
        creator: &docker,
        lifecycle: &docker,
        exec: &docker,
        uploader: &docker,
        host_runner: &host_runner,
        token_client: token_client.as_ref(),
        clock: &clock,
        env: &env,
        runtime: runtime.handle(),
    };
    runner.run(config, request)
}

/// Validate request and configuration before any engine or network work.
fn validate_run_inputs(config: &AppConfig, request: &RunRequest) -> PodbotResult<()> {
    crate::api::validate_run_request_for_agent(request)?;
    crate::api::validate_github_config_for_run(config, request)?;
    if needs_github_token(config) {
        config.github.validate()?;
    }
    Ok(())
}

/// Build the GitHub token client when the workspace is cloned.
fn build_token_client(
    config: &AppConfig,
    runtime: &tokio::runtime::Runtime,
) -> PodbotResult<Option<OctocrabAppClient>> {
    if !needs_github_token(config) {
        return Ok(None);
    }
    config.github.validate()?;
    let (app_id, _installation_id, private_key_path) =
        crate::api::token_daemon::github_credentials(config)?;
    let private_key = load_private_key(private_key_path)?;
    let octocrab = runtime.block_on(async { build_app_client(app_id, private_key) })?;
    Ok(Some(OctocrabAppClient::new(octocrab)))
}
