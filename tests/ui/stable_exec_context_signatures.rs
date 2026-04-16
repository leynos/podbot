use podbot::api::{CommandOutcome, ExecContext, ExecRequest};
use podbot::config::AppConfig;

fn main() {
    let _connect: fn(&AppConfig, &tokio::runtime::Handle) -> podbot::error::Result<ExecContext> =
        ExecContext::connect;
    let _exec: fn(&ExecContext, &ExecRequest) -> podbot::error::Result<CommandOutcome> =
        ExecContext::exec;
}
