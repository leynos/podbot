//! Host-style integration tests for the documented stable library boundary.

use podbot::api::{CommandOutcome, ExecContext, ExecMode, ExecRequest};
use podbot::config::AppConfig;
use rstest::rstest;

#[rstest]
fn stable_embedder_path_uses_only_supported_modules() {
    let request = ExecRequest::new("sandbox", vec![String::from("echo"), String::from("hello")])
        .expect("request should be valid")
        .with_mode(ExecMode::Protocol)
        .with_tty(true);
    let serialized = serde_json::to_string(&request).expect("request should serialize");
    let round_trip: ExecRequest =
        serde_json::from_str(&serialized).expect("request should deserialize");

    assert_eq!(round_trip, request);
    assert_eq!(round_trip.mode(), ExecMode::Protocol);
    assert!(!request.tty());
    assert!(!round_trip.tty());

    let connect_signature: fn(
        &AppConfig,
        &tokio::runtime::Handle,
    ) -> podbot::error::Result<ExecContext> = ExecContext::connect;
    let exec_signature: fn(&ExecContext, &ExecRequest) -> podbot::error::Result<CommandOutcome> =
        ExecContext::exec;

    assert!(std::ptr::fn_addr_eq(
        connect_signature,
        ExecContext::connect
            as fn(&AppConfig, &tokio::runtime::Handle) -> podbot::error::Result<ExecContext>,
    ));
    assert!(std::ptr::fn_addr_eq(
        exec_signature,
        ExecContext::exec
            as fn(&ExecContext, &ExecRequest) -> podbot::error::Result<CommandOutcome>,
    ));
}
