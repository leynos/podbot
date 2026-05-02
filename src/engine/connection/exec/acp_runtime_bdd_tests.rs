//! BDD scenario wiring for the Agentic Control Protocol (ACP) runtime
//! method denylist feature.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use ortho_config::serde_json::{self, Value};
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};
use tokio::io::AsyncWrite;
use tokio::sync::mpsc;

use super::{
    OutboundFrameAssembler, OutboundPolicyAdapter, SINK_CHANNEL_CAPACITY, WriteCmd,
};
use crate::engine::connection::exec::protocol::acp_policy::MethodDenylist;

type StepResult<T> = Result<T, String>;

#[derive(Default, Clone)]
struct RecordingHostStdout {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl RecordingHostStdout {
    fn snapshot(&self) -> Result<Vec<u8>, String> {
        self.bytes
            .lock()
            .map(|guard| guard.clone())
            .map_err(|err| format!("recording writer mutex poisoned: {err}"))
    }
}

impl AsyncWrite for RecordingHostStdout {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.bytes.lock() {
            Ok(mut guard) => {
                guard.extend_from_slice(buf);
                Poll::Ready(Ok(buf.len()))
            }
            Err(_) => Poll::Ready(Err(io::Error::other("recording writer mutex poisoned"))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[derive(Default, ScenarioState)]
struct DenylistState {
    host_stdout_bytes: Slot<Vec<u8>>,
    sink_commands: Slot<Vec<WriteCmd>>,
}

#[rstest::fixture]
fn denylist_state() -> DenylistState {
    DenylistState::default()
}

fn permitted_request_frame(method: &str, id: i64) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": {},
    }))
    .expect("permitted request serializes");
    bytes.push(b'\n');
    bytes
}

fn blocked_request_frame(method: &str, id: i64) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": {},
    }))
    .expect("blocked request serializes");
    bytes.push(b'\n');
    bytes
}

fn blocked_notification_frame(method: &str) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": {},
    }))
    .expect("blocked notification serializes");
    bytes.push(b'\n');
    bytes
}

fn build_runtime() -> (OutboundPolicyAdapter, mpsc::Receiver<WriteCmd>, RecordingHostStdout) {
    let (sender, receiver) = mpsc::channel::<WriteCmd>(SINK_CHANNEL_CAPACITY);
    let assembler = OutboundFrameAssembler::new(MethodDenylist::default_families());
    let adapter = OutboundPolicyAdapter::new(assembler, sender, "container-bdd");
    (adapter, receiver, RecordingHostStdout::default())
}

async fn drain_sink(mut receiver: mpsc::Receiver<WriteCmd>) -> Vec<WriteCmd> {
    receiver.close();
    let mut commands = Vec::new();
    while let Some(cmd) = receiver.recv().await {
        commands.push(cmd);
    }
    commands
}

fn run_runtime<F>(
    state: &DenylistState,
    chunks: Vec<Vec<u8>>,
    finalize: F,
) -> StepResult<()>
where
    F: FnOnce(),
{
    finalize();
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|err| format!("could not build runtime: {err}"))?;
    let (commands, host_bytes) = runtime.block_on(async move {
        let (mut adapter, receiver, host_stdout) = build_runtime();
        let host_stdout_handle = host_stdout.clone();
        let mut writer = host_stdout;
        for chunk in chunks {
            adapter
                .handle_chunk(&chunk, &mut writer)
                .await
                .expect("chunk handles cleanly");
        }
        adapter.finish();
        drop(adapter);
        let commands = drain_sink(receiver).await;
        let bytes = host_stdout_handle
            .snapshot()
            .expect("host stdout snapshot should succeed");
        (commands, bytes)
    });
    state.host_stdout_bytes.set(host_bytes);
    state.sink_commands.set(commands);
    Ok(())
}

#[given("the ACP runtime adapter is configured with the default denylist")]
fn adapter_uses_default_denylist(denylist_state: &DenylistState) {
    let _ = denylist_state;
}

#[when(r#"the agent emits a "terminal/create" request with id 7"#)]
fn emit_blocked_terminal_create(denylist_state: &DenylistState) -> StepResult<()> {
    let frame = blocked_request_frame("terminal/create", 7);
    run_runtime(denylist_state, vec![frame], || {})
}

#[when(r#"the agent emits a "session/new" request with id 1"#)]
fn emit_permitted_session_new(denylist_state: &DenylistState) -> StepResult<()> {
    let frame = permitted_request_frame("session/new", 1);
    run_runtime(denylist_state, vec![frame], || {})
}

#[when(r#"the agent emits an "fs/changed" notification"#)]
fn emit_blocked_notification(denylist_state: &DenylistState) -> StepResult<()> {
    let frame = blocked_notification_frame("fs/changed");
    run_runtime(denylist_state, vec![frame], || {})
}

#[when("the agent emits a blocked frame split across two output chunks")]
fn emit_blocked_split(denylist_state: &DenylistState) -> StepResult<()> {
    let frame = blocked_request_frame("terminal/create", 2);
    let split_at = frame.len() / 2;
    let first = frame
        .get(..split_at)
        .ok_or_else(|| String::from("split prefix missing"))?
        .to_vec();
    let second = frame
        .get(split_at..)
        .ok_or_else(|| String::from("split suffix missing"))?
        .to_vec();
    run_runtime(denylist_state, vec![first, second], || {})
}

#[when("the agent emits a blocked request followed by a permitted request")]
fn emit_blocked_then_permitted(denylist_state: &DenylistState) -> StepResult<()> {
    let mut chunk = blocked_request_frame("terminal/create", 5);
    let permitted = permitted_request_frame("session/update", 6);
    chunk.extend_from_slice(&permitted);
    run_runtime(denylist_state, vec![chunk], || {})
}

#[then("host stdout receives no bytes from the blocked request")]
fn assert_host_stdout_empty(denylist_state: &DenylistState) -> StepResult<()> {
    let bytes = denylist_state
        .host_stdout_bytes
        .get()
        .ok_or_else(|| String::from("host stdout snapshot not recorded"))?;
    if bytes.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "expected host stdout to be empty, got {len} bytes",
            len = bytes.len(),
        ))
    }
}

#[then("host stdout receives no bytes from the blocked notification")]
fn assert_host_stdout_empty_for_notification(denylist_state: &DenylistState) -> StepResult<()> {
    assert_host_stdout_empty(denylist_state)
}

#[then("container stdin receives a synthesized JSON-RPC error with id 7")]
fn assert_synthesized_id_seven(denylist_state: &DenylistState) -> StepResult<()> {
    expect_synthesized_id(denylist_state, &serde_json::json!(7))
}

#[then("container stdin receives a synthesized JSON-RPC error with id 2")]
fn assert_synthesized_id_two(denylist_state: &DenylistState) -> StepResult<()> {
    expect_synthesized_id(denylist_state, &serde_json::json!(2))
}

#[then("container stdin receives a synthesized JSON-RPC error for the blocked request")]
fn assert_synthesized_for_blocked(denylist_state: &DenylistState) -> StepResult<()> {
    expect_synthesized_id(denylist_state, &serde_json::json!(5))
}

fn expect_synthesized_id(denylist_state: &DenylistState, expected_id: &Value) -> StepResult<()> {
    let commands = denylist_state
        .sink_commands
        .get()
        .ok_or_else(|| String::from("sink command snapshot not recorded"))?;
    let synthesised: Vec<&Vec<u8>> = commands
        .iter()
        .filter_map(|cmd| match cmd {
            WriteCmd::Synthesised(bytes) => Some(bytes),
            WriteCmd::Forward(_) => None,
        })
        .collect();
    let bytes = match synthesised.as_slice() {
        [bytes] => *bytes,
        other => {
            return Err(format!(
                "expected exactly one synthesized response, got {count}",
                count = other.len(),
            ));
        }
    };
    let payload = bytes
        .strip_suffix(b"\n")
        .ok_or_else(|| String::from("synthesized response should end in newline"))?;
    let parsed: Value = serde_json::from_slice(payload)
        .map_err(|err| format!("synthesized response did not parse: {err}"))?;
    let id = parsed
        .get("id")
        .ok_or_else(|| String::from("synthesized response missing id"))?;
    if id == expected_id {
        Ok(())
    } else {
        Err(format!("expected id {expected_id}, got {id}"))
    }
}

#[then("host stdout receives the permitted frame verbatim")]
fn assert_host_stdout_matches_permitted(denylist_state: &DenylistState) -> StepResult<()> {
    let bytes = denylist_state
        .host_stdout_bytes
        .get()
        .ok_or_else(|| String::from("host stdout snapshot not recorded"))?;
    let expected = permitted_request_frame("session/new", 1);
    if bytes == expected {
        Ok(())
    } else {
        Err(String::from(
            "host stdout did not receive the permitted frame verbatim",
        ))
    }
}

#[then("host stdout receives only the permitted frame verbatim")]
fn assert_host_stdout_matches_permitted_after_blocked(
    denylist_state: &DenylistState,
) -> StepResult<()> {
    let bytes = denylist_state
        .host_stdout_bytes
        .get()
        .ok_or_else(|| String::from("host stdout snapshot not recorded"))?;
    let expected = permitted_request_frame("session/update", 6);
    if bytes == expected {
        Ok(())
    } else {
        Err(String::from(
            "host stdout should contain only the permitted frame after a blocked one",
        ))
    }
}

#[then("container stdin receives no synthesized response")]
fn assert_no_synthesized(denylist_state: &DenylistState) -> StepResult<()> {
    let commands = denylist_state
        .sink_commands
        .get()
        .ok_or_else(|| String::from("sink command snapshot not recorded"))?;
    let any_synthesised = commands
        .iter()
        .any(|cmd| matches!(cmd, WriteCmd::Synthesised(_)));
    if any_synthesised {
        Err(String::from(
            "expected no synthesized response on container stdin",
        ))
    } else {
        Ok(())
    }
}

#[scenario(
    path = "tests/features/acp_method_denylist.feature",
    name = "Blocked request returns a synthesized error and is not forwarded"
)]
fn blocked_request_returns_synthesized_error(denylist_state: DenylistState) {
    let _ = denylist_state;
}

#[scenario(
    path = "tests/features/acp_method_denylist.feature",
    name = "Permitted method passes through unchanged byte-for-byte"
)]
fn permitted_method_passes_through(denylist_state: DenylistState) {
    let _ = denylist_state;
}

#[scenario(
    path = "tests/features/acp_method_denylist.feature",
    name = "Blocked notification is dropped silently"
)]
fn blocked_notification_dropped(denylist_state: DenylistState) {
    let _ = denylist_state;
}

#[scenario(
    path = "tests/features/acp_method_denylist.feature",
    name = "Frame split across two chunks reassembles before the policy applies"
)]
fn split_frame_reassembles(denylist_state: DenylistState) {
    let _ = denylist_state;
}

#[scenario(
    path = "tests/features/acp_method_denylist.feature",
    name = "Permitted frame following a blocked frame still flushes correctly"
)]
fn permitted_after_blocked(denylist_state: DenylistState) {
    let _ = denylist_state;
}
