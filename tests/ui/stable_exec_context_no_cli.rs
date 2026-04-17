//! Compile-pass lock for the stable `ExecContext` API when the `cli` feature
//! is disabled.
//!
//! This fixture ensures the stable embedding surface remains available in
//! library-only builds that omit `podbot::cli`.

use podbot::api::{CommandOutcome, ExecContext, ExecMode, ExecRequest};
use podbot::config::AppConfig;
use podbot::error::PodbotError;

fn main() -> Result<(), PodbotError> {
    let _connect: fn(&AppConfig, &tokio::runtime::Handle) -> podbot::error::Result<ExecContext> =
        ExecContext::connect;
    let _exec: fn(&ExecContext, &ExecRequest) -> podbot::error::Result<CommandOutcome> =
        ExecContext::exec;

    let _request = ExecRequest::new("sandbox", vec![String::from("echo")])?
        .with_mode(ExecMode::Detached)
        .with_tty(true);

    Ok(())
}
