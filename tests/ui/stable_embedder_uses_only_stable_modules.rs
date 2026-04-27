//! Compile-time verification that an embedder using only the stable modules
//! (`podbot::api`, `podbot::config`, `podbot::error`) can express the full
//! exec workflow without importing engine, CLI, or GitHub internals.
//!
//! Run via `cargo test` — trybuild compiles this file in isolation and fails
//! the test if it does not compile.

use podbot::api::{CommandOutcome, ExecContext, ExecMode, ExecRequest, exec};
use podbot::config::AppConfig;
use podbot::error::Result;

fn _assert_exec_surface_is_importable(handle: &tokio::runtime::Handle) -> Result<ExecContext> {
    let _request = ExecRequest::new("c", vec![String::from("cmd")])?
        .with_mode(ExecMode::Attached)
        .with_tty(false);
    let config = AppConfig::default();
    ExecContext::connect(&config, handle)
}

fn _assert_top_level_exec_is_importable(
    config: &AppConfig,
    request: &ExecRequest,
) -> Result<CommandOutcome> {
    exec(config, request)
}

fn _assert_context_exec_is_importable(
    context: &ExecContext,
    request: &ExecRequest,
) -> Result<CommandOutcome> {
    context.exec(request)
}

fn main() {}
