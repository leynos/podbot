//! Verifies that `CapabilityPolicy` selects raw forwarding or runtime
//! enforcement for outbound ACP frames.

use rstest::rstest;

use crate::engine::connection::exec::acp_test_support::{jsonrpc_frame, run_policy_session};
use crate::engine::connection::exec::session::CapabilityPolicy;

const ROUTING_CONTAINER_ID: &str = "capability-policy-routing";

fn blocked_terminal_create_frame() -> Result<Vec<u8>, serde_json::Error> {
    jsonrpc_frame(Some(&serde_json::json!(7)), "terminal/create", b"\n")
}

/// Pure query: returns `true` when `bytes` parses as the synthesized denial
/// response for the blocked `terminal/create` request with id 7.
fn synthesized_response_for_terminal_create(bytes: &[u8]) -> bool {
    let Ok(response) =
        serde_json::from_slice::<serde_json::Value>(bytes.strip_suffix(b"\n").unwrap_or(bytes))
    else {
        return false;
    };
    response.get("id") == Some(&serde_json::json!(7))
        && response
            .get("error")
            .and_then(|error| error.get("data"))
            .and_then(|data| data.get("method"))
            == Some(&serde_json::json!("terminal/create"))
}

#[rstest]
#[case::mask_and_deny_routes_through_enforcement_path(CapabilityPolicy::MaskAndDeny, false, true)]
#[case::disabled_policy_forwards_all_frames_raw(CapabilityPolicy::Disabled, true, false)]
#[case::mask_only_policy_forwards_blocked_frames_raw(CapabilityPolicy::MaskOnly, true, false)]
fn routes_output_frame_for_capability_policy(
    #[case] policy: CapabilityPolicy,
    #[case] expect_forward_raw: bool,
    #[case] expect_synthesized_response: bool,
) {
    let frame = blocked_terminal_create_frame().expect("blocked request should serialize");
    let captured = run_policy_session(ROUTING_CONTAINER_ID, policy, &[], &frame)
        .expect("policy session should complete");

    if expect_forward_raw {
        assert_eq!(
            captured.host_stdout, frame,
            "{policy:?} should preserve the byte-transparent output path",
        );
    } else {
        assert_ne!(
            captured.host_stdout, frame,
            "{policy:?} must not forward blocked frames verbatim",
        );
    }
    if expect_synthesized_response {
        assert!(
            synthesized_response_for_terminal_create(&captured.container_stdin),
            "{policy:?} should write a synthesized denial response to container stdin",
        );
    } else {
        assert!(
            captured.container_stdin.is_empty(),
            "{policy:?} should not write a synthesized denial response",
        );
    }
}
