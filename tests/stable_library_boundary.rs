//! Host-style integration tests for the documented stable library boundary.

use podbot::api::{ExecMode, ExecRequest};
use rstest::rstest;

#[rstest]
fn exec_request_serialisation_round_trips_and_enforces_tty_normalisation() {
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
}
