//! Unit tests for `mask_acp_initialize_frame`, covering blocked-capability
//! removal, preservation of unrelated capabilities, and pass-through of
//! non-initialize or malformed frames.

use rstest::rstest;

use super::{
    check_masked_client_capabilities, client_capabilities, initialize_frame,
    initialize_frame_with_capabilities, initialize_with_only_blocked_capabilities,
    malformed_initialize_bytes, mask_acp_initialize_frame, params, parse_frame_payload,
    session_new_bytes, split_frame_line_ending,
};

#[rstest]
#[case("\n")]
#[case("\r\n")]
fn mask_acp_initialize_frame_removes_blocked_capabilities(#[case] line_ending: &str) {
    let frame = initialize_frame(line_ending).expect("initialize frame should serialize");
    let masked = mask_acp_initialize_frame(&frame);
    let payload = parse_frame_payload(&masked).expect("frame should contain JSON payload");

    assert_eq!(
        split_frame_line_ending(&masked).1,
        line_ending.as_bytes(),
        "line ending should be preserved"
    );
    check_masked_client_capabilities(&payload)
        .expect("blocked client capabilities should be masked");
}

#[test]
fn mask_acp_initialize_frame_removes_empty_client_capabilities() {
    let frame =
        initialize_with_only_blocked_capabilities("\n").expect("initialize frame should serialize");
    let masked = mask_acp_initialize_frame(&frame);
    let payload = parse_frame_payload(&masked).expect("frame should contain JSON payload");
    let masked_params = params(&payload).expect("initialize params should remain present");

    assert!(
        !masked_params.contains_key("clientCapabilities"),
        "clientCapabilities should be removed when all entries are masked"
    );
    assert_eq!(
        masked_params.get("protocolVersion"),
        Some(&serde_json::json!(1)),
        "protocolVersion should remain unchanged"
    );
    assert_eq!(
        masked_params.get("clientInfo"),
        Some(&serde_json::json!({
            "name": "podbot-tests",
            "version": "1.0.0"
        })),
        "clientInfo should remain unchanged"
    );
}

#[rstest]
#[case(
    serde_json::json!({
        "fs": { "readTextFile": true },
        "auth": { "token": true }
    }),
    &["fs"],
    &["auth"]
)]
#[case(
    serde_json::json!({
        "terminal": true,
        "logging": { "level": "info" }
    }),
    &["terminal"],
    &["logging"]
)]
#[case(
    serde_json::json!({
        "fs": { "readTextFile": true },
        "terminal": true,
        "auth": { "token": true },
        "logging": { "level": "debug" }
    }),
    &["fs", "terminal"],
    &["auth", "logging"]
)]
fn mask_acp_initialize_frame_preserves_unrelated_capabilities(
    #[case] capabilities: serde_json::Value,
    #[case] removed_capabilities: &[&str],
    #[case] preserved_capabilities: &[&str],
) {
    let frame = initialize_frame_with_capabilities(&capabilities, "\n")
        .expect("initialize frame should serialize");
    let masked = mask_acp_initialize_frame(&frame);
    let result = parse_frame_payload(&masked).expect("frame should contain JSON payload");
    let caps = client_capabilities(&result).expect("clientCapabilities should remain");

    for capability in removed_capabilities {
        assert!(
            !caps.contains_key(*capability),
            "{capability} should be removed"
        );
    }
    for capability in preserved_capabilities {
        assert!(
            caps.contains_key(*capability),
            "{capability} should be preserved"
        );
    }
}

#[test]
fn mask_acp_initialize_frame_passes_through_frame_without_line_ending() {
    let frame =
        initialize_with_only_blocked_capabilities("").expect("initialize frame should serialize");
    let masked = mask_acp_initialize_frame(&frame);

    assert!(
        !masked.ends_with(b"\n"),
        "masked frame should not gain a trailing newline"
    );
    let result: serde_json::Value =
        serde_json::from_slice(&masked).expect("result should be valid JSON");
    let masked_params = params(&result).expect("params should remain");
    assert!(
        masked_params.get("clientCapabilities").is_none(),
        "capabilities should still be masked even without a line ending"
    );
}

#[test]
fn mask_acp_initialize_frame_leaves_non_initialize_messages_unchanged() {
    let mut frame = session_new_bytes();
    frame.push(b'\n');

    assert_eq!(mask_acp_initialize_frame(&frame), frame);
}

#[test]
fn mask_acp_initialize_frame_leaves_malformed_input_unchanged() {
    let frame = malformed_initialize_bytes();

    assert_eq!(mask_acp_initialize_frame(&frame), frame);
}
