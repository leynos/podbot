//! Pure policy decisions for Agentic Control Protocol (ACP) runtime
//! enforcement.
//!
//! This module owns the value types and pure functions that decide whether an
//! agent-emitted ACP JavaScript Object Notation Remote Procedure Call
//! (JSON-RPC) frame must be blocked. It deliberately depends on neither
//! `tokio` nor `tracing`; the runtime adapter in `acp_runtime` is responsible
//! for I/O, channel sends, and observability so the policy layer remains
//! trivially testable.
//!
//! The policy is intentionally tolerant: any frame that fails to parse, lacks
//! a `method` field, or carries an unblocked method passes through unchanged.
//! This mirrors the first-frame masking philosophy established in
//! `acp_helpers` and protects non-Agentic Control Protocol traffic.

use ortho_config::serde_json::{self, Value};

use super::acp_helpers::split_frame_line_ending;

/// A capability family blocked by Podbot at runtime.
///
/// Each family is identified by a method-name prefix terminated by the ACP
/// family separator `/`. Matching requires the prefix and at least one
/// non-empty operation name following it, so `terminal/` matches
/// `terminal/create` but not the literal `terminal/` (which carries no
/// operation) or an unrelated method like `terminalize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MethodFamily {
    /// The capability prefix, including the trailing `/` separator.
    pub(crate) prefix: &'static str,
}

impl MethodFamily {
    /// Return `true` when `method` belongs to this family.
    pub(crate) fn matches(&self, method: &str) -> bool {
        method
            .strip_prefix(self.prefix)
            .is_some_and(|rest| !rest.is_empty())
    }
}

/// The default set of ACP capability families that Podbot blocks at runtime.
///
/// `terminal/` and `fs/` correspond to the same families that the
/// initialization-time masker strips from `clientCapabilities` advertisements,
/// so the runtime denylist closes the symmetric door on agent-emitted method
/// calls in those families.
pub(crate) const DEFAULT_BLOCKED_FAMILIES: &[MethodFamily] = &[
    MethodFamily { prefix: "terminal/" },
    MethodFamily { prefix: "fs/" },
];

/// A static set of [`MethodFamily`] values that Podbot refuses to forward.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MethodDenylist {
    families: &'static [MethodFamily],
}

impl MethodDenylist {
    /// Construct a denylist over the supplied families.
    pub(crate) const fn new(families: &'static [MethodFamily]) -> Self {
        Self { families }
    }

    /// Construct the default denylist covering `terminal/` and `fs/`.
    pub(crate) const fn default_families() -> Self {
        Self::new(DEFAULT_BLOCKED_FAMILIES)
    }

    /// Return `true` when `method` belongs to any family in the denylist.
    pub(crate) fn is_blocked(&self, method: &str) -> bool {
        self.families.iter().any(|family| family.matches(method))
    }
}

/// The decision returned by [`evaluate_agent_outbound_frame`].
///
/// `Forward` means the byte-identical frame should be relayed to the host.
/// `BlockNotification` and `BlockRequest` both cause the frame to be dropped;
/// `BlockRequest` additionally carries the original JSON-RPC `id` so the
/// runtime adapter can synthesize a matching error response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FrameDecision {
    /// Forward the original bytes unchanged.
    Forward,
    /// Drop a blocked notification (no `id` field present).
    BlockNotification {
        /// The blocked method name from the JSON-RPC frame.
        method: String,
    },
    /// Drop a blocked request and prepare a synthesized error response.
    BlockRequest {
        /// The original JSON-RPC `id`, type-preserved as the source value.
        id: Value,
        /// The blocked method name from the JSON-RPC frame.
        method: String,
    },
}

/// Decide whether `frame` (an agent-outbound JSON-RPC frame) should be
/// forwarded to the host or blocked by `denylist`.
///
/// On any parse failure, missing-method, or response/batch shape, the
/// function returns [`FrameDecision::Forward`]. Only a JSON-RPC request or
/// notification whose `method` belongs to a blocked family produces a
/// [`FrameDecision::BlockRequest`] or [`FrameDecision::BlockNotification`].
pub(crate) fn evaluate_agent_outbound_frame(
    frame: &[u8],
    denylist: &MethodDenylist,
) -> FrameDecision {
    let (payload, _line_ending) = split_frame_line_ending(frame);
    let Ok(message) = serde_json::from_slice::<Value>(payload) else {
        return FrameDecision::Forward;
    };
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return FrameDecision::Forward;
    };
    if !denylist.is_blocked(method) {
        return FrameDecision::Forward;
    }
    let method_owned = String::from(method);
    match message.get("id") {
        Some(id) => FrameDecision::BlockRequest {
            id: id.clone(),
            method: method_owned,
        },
        None => FrameDecision::BlockNotification {
            method: method_owned,
        },
    }
}

/// JSON-RPC application error code used by Podbot for capability-policy
/// denials.
///
/// The value lives in the JSON-RPC application-defined range
/// `-32099..=-32000` and avoids collision with `-32601 Method not found`,
/// which would imply the method is unknown to the server. Reserve `-32002`
/// for the future "override required" follow-on (Step 2.6.4).
pub(crate) const METHOD_BLOCKED_ERROR_CODE: i64 = -32_001;

/// Stable error message body for capability-policy denials. The synthesized
/// JSON-RPC response also carries a structured `data.reason` discriminator
/// so agents can branch on `reason` rather than parsing the message string.
pub(crate) const METHOD_BLOCKED_ERROR_MESSAGE: &str =
    "Method blocked by Podbot ACP capability policy";

/// Stable `data.reason` discriminator embedded in synthesized error
/// responses.
pub(crate) const METHOD_BLOCKED_ERROR_REASON: &str = "podbot_capability_policy";

/// Build a JSON-RPC 2.0 error response for a blocked request.
///
/// The original `id` is preserved as a [`Value`] so numeric, string, and null
/// identifiers retain their JSON type. The supplied `line_ending` bytes are
/// appended verbatim (use `b"\n"` when the original frame carried no
/// recognized line terminator).
///
/// # Errors
///
/// Returns the underlying `serde_json` error if serialization of the
/// constructed payload fails. In practice this cannot happen because every
/// component is a finite, non-recursive [`Value`] built from owned primitives,
/// but the caller propagates the error rather than panicking on the
/// theoretical edge case.
pub(crate) fn build_method_blocked_error(
    id: &Value,
    method: &str,
    line_ending: &[u8],
) -> serde_json::Result<Vec<u8>> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": METHOD_BLOCKED_ERROR_CODE,
            "message": METHOD_BLOCKED_ERROR_MESSAGE,
            "data": {
                "method": method,
                "reason": METHOD_BLOCKED_ERROR_REASON,
            },
        },
    });
    let mut serialized = serde_json::to_vec(&payload)?;
    serialized.extend_from_slice(line_ending);
    Ok(serialized)
}

#[cfg(test)]
#[path = "acp_policy_tests.rs"]
mod tests;
