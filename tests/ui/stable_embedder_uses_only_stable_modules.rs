//! Compile-time verification that an embedder using only the stable modules
//! (`podbot::api`, `podbot::config`, `podbot::error`) can express the full
//! exec workflow without importing engine, CLI, or GitHub internals.
//!
//! Run via `cargo test` — trybuild compiles this file in isolation and fails
//! the test if it does not compile.

use podbot::api::{CommandOutcome, ExecContext, ExecMode, ExecRequest, RunRequest, exec};
use podbot::config::AppConfig;
use podbot::error::Result;

fn main() -> Result<()> {
    let _connect: fn(&AppConfig, &tokio::runtime::Handle) -> Result<ExecContext> =
        ExecContext::connect;
    let _context_exec: fn(&ExecContext, &ExecRequest) -> Result<CommandOutcome> =
        ExecContext::exec;
    let _top_level_exec: fn(&AppConfig, &ExecRequest) -> Result<CommandOutcome> = exec;
    let _run_request_new: fn(String, String) -> Result<RunRequest> = RunRequest::new;

    let _exec_request = ExecRequest::new("c", vec![String::from("cmd")])?
        .with_mode(ExecMode::Attached)
        .with_tty(false);
    let _run_request = RunRequest::new("owner/name", "main")?;

    Ok(())
}
