//! Unit tests for the pure Agentic Control Protocol (ACP) policy module.
//!
//! These tests cover the family-prefix matcher, the per-frame decision
//! function, and the synthesized error builder. They use only standard
//! library and `serde_json` types so the policy layer remains trivially
//! testable without `tokio` or `tracing`.

use ortho_config::serde_json::{self, Value};
use rstest::rstest;

use super::{
    DEFAULT_BLOCKED_FAMILIES, FrameDecision, METHOD_BLOCKED_ERROR_CODE,
    METHOD_BLOCKED_ERROR_MESSAGE, METHOD_BLOCKED_ERROR_REASON, MethodDenylist, MethodFamily,
    build_method_blocked_error, evaluate_agent_outbound_frame,
};

const TERMINAL_FAMILY: MethodFamily = MethodFamily {
    prefix: "terminal/",
};

#[rstest]
#[case::exact_prefix_with_operation("terminal/create", true)]
#[case::nested_path("terminal/output/follow", true)]
#[case::bare_prefix_no_operation("terminal/", false)]
#[case::similar_word_no_slash("terminalize", false)]
#[case::word_without_separator("terminal", false)]
#[case::unrelated("session/new", false)]
fn method_family_matches_with_slash_boundary(#[case] method: &str, #[case] expected: bool) {
    assert_eq!(TERMINAL_FAMILY.matches(method), expected);
}

#[rstest]
#[case("terminal/create", true)]
#[case("fs/read_text_file", true)]
#[case("fs/write_text_file", true)]
#[case("session/new", false)]
#[case("initialize", false)]
fn default_denylist_blocks_terminal_and_fs_only(#[case] method: &str, #[case] expected: bool) {
    let denylist = MethodDenylist::default_families();
    assert_eq!(denylist.is_blocked(method), expected);
}

fn jsonrpc_request(id: &Value, method: &str) -> Vec<u8> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": {},
    });
    let mut bytes = serde_json::to_vec(&payload).expect("request serializes");
    bytes.push(b'\n');
    bytes
}

fn jsonrpc_notification(method: &str) -> Vec<u8> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": {},
    });
    let mut bytes = serde_json::to_vec(&payload).expect("notification serializes");
    bytes.push(b'\n');
    bytes
}

#[rstest]
#[case::numeric_id(serde_json::json!(7))]
#[case::string_id(serde_json::json!("call-7"))]
#[case::null_id(serde_json::json!(null))]
fn evaluate_returns_block_request_with_preserved_id(#[case] id: Value) {
    let frame = jsonrpc_request(&id, "terminal/create");
    let denylist = MethodDenylist::default_families();

    let decision = evaluate_agent_outbound_frame(&frame, &denylist);

    match decision {
        FrameDecision::BlockRequest {
            id: actual_id,
            method,
        } => {
            assert_eq!(actual_id, id, "request id should be preserved verbatim");
            assert_eq!(method, "terminal/create");
        }
        other => panic!("expected BlockRequest, got {other:?}"),
    }
}

#[test]
fn evaluate_returns_block_notification_for_blocked_method_without_id() {
    let frame = jsonrpc_notification("fs/changed");
    let denylist = MethodDenylist::default_families();

    let decision = evaluate_agent_outbound_frame(&frame, &denylist);

    assert_eq!(
        decision,
        FrameDecision::BlockNotification {
            method: String::from("fs/changed"),
        }
    );
}

#[test]
fn evaluate_forwards_permitted_request() {
    let frame = jsonrpc_request(&serde_json::json!(1), "session/new");
    let denylist = MethodDenylist::default_families();

    assert_eq!(
        evaluate_agent_outbound_frame(&frame, &denylist),
        FrameDecision::Forward
    );
}

#[test]
fn evaluate_forwards_malformed_json() {
    let frame: &[u8] = br#"{"jsonrpc":"2.0","method":"terminal/create" "#;
    let denylist = MethodDenylist::default_families();

    assert_eq!(
        evaluate_agent_outbound_frame(frame, &denylist),
        FrameDecision::Forward,
        "malformed JSON should pass through unchanged"
    );
}

#[test]
fn evaluate_forwards_response_without_method_field() {
    let frame = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {"value": 42},
    }))
    .expect("response serializes");
    let denylist = MethodDenylist::default_families();

    assert_eq!(
        evaluate_agent_outbound_frame(&frame, &denylist),
        FrameDecision::Forward
    );
}

#[test]
fn evaluate_forwards_frame_when_method_not_string() {
    let frame = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": 7,
    }))
    .expect("invalid frame still serializes");
    let denylist = MethodDenylist::default_families();

    assert_eq!(
        evaluate_agent_outbound_frame(&frame, &denylist),
        FrameDecision::Forward
    );
}

#[test]
fn evaluate_handles_crlf_terminated_frame() {
    let mut frame = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "terminal/create",
    }))
    .expect("request serializes");
    frame.extend_from_slice(b"\r\n");
    let denylist = MethodDenylist::default_families();

    let decision = evaluate_agent_outbound_frame(&frame, &denylist);

    assert!(matches!(decision, FrameDecision::BlockRequest { .. }));
}

#[rstest]
#[case::numeric(serde_json::json!(42), b"\n" as &[u8])]
#[case::string(serde_json::json!("call-42"), b"\n" as &[u8])]
#[case::null(serde_json::json!(null), b"\n" as &[u8])]
#[case::crlf(serde_json::json!(1), b"\r\n" as &[u8])]
fn build_error_payload_round_trips_with_expected_fields(
    #[case] id: Value,
    #[case] line_ending: &[u8],
) {
    let bytes = build_method_blocked_error(&id, "terminal/create", line_ending)
        .expect("error payload should serialize");

    assert!(
        bytes.ends_with(line_ending),
        "supplied line ending should be preserved"
    );

    let payload_end = bytes
        .len()
        .checked_sub(line_ending.len())
        .expect("payload must precede the line ending");
    let payload_slice = bytes
        .get(..payload_end)
        .expect("payload prefix must exist before the line ending");
    let parsed: Value = serde_json::from_slice(payload_slice)
        .expect("synthesized error must round-trip through serde_json");
    assert_eq!(parsed.get("jsonrpc"), Some(&Value::from("2.0")));
    assert_eq!(parsed.get("id"), Some(&id));
    let error = parsed
        .get("error")
        .and_then(Value::as_object)
        .expect("error object must be present");
    assert_eq!(
        error.get("code"),
        Some(&Value::from(METHOD_BLOCKED_ERROR_CODE)),
    );
    assert_eq!(
        error.get("message"),
        Some(&Value::from(METHOD_BLOCKED_ERROR_MESSAGE)),
    );
    let data = error
        .get("data")
        .and_then(Value::as_object)
        .expect("error.data object must be present");
    assert_eq!(data.get("method"), Some(&Value::from("terminal/create")));
    assert_eq!(
        data.get("reason"),
        Some(&Value::from(METHOD_BLOCKED_ERROR_REASON)),
    );
}

#[test]
fn default_blocked_families_match_design_decision() {
    let prefixes: Vec<&str> = DEFAULT_BLOCKED_FAMILIES
        .iter()
        .map(|family| family.prefix)
        .collect();
    assert_eq!(prefixes, vec!["terminal/", "fs/"]);
}
