//! Runtime adapter and container-stdin sink for Agentic Control Protocol
//! (ACP) capability enforcement.
//!
//! The pure policy in `acp_policy` and the streaming framer in `acp_frame`
//! decide what each agent-emitted JSON-RPC frame should become. This module
//! turns those decisions into actual I/O: it writes permitted frames to host
//! stdout verbatim, synthesizes JSON-RPC error responses for blocked
//! requests, and emits one stderr `tracing::warn!` per denial or
//! once-per-session fallback record.
//!
//! ## Sink task model
//!
//! Container stdin has a single owner: a dedicated [`run_container_stdin_sink`]
//! task that drains a bounded [`tokio::sync::mpsc::Receiver`] of [`WriteCmd`].
//! Both the host-stdin forwarder and the [`OutboundPolicyAdapter`] become
//! senders. This eliminates the [`super::STDIN_SETTLE_TIMEOUT`] race that a
//! shared writer would have created and lets the adapter guarantee that
//! synthesized error responses reach the agent before the input stream
//! closes.
//!
//! Ordering invariant: the protocol coordinator drops every sender (the
//! [`OutboundPolicyAdapter`] and the host-stdin forwarder) only after the
//! output stream has fully drained and after all blocked-request decisions
//! have been queued. Because the channel preserves send order and the sink
//! processes commands sequentially before `recv()` returns `None` for the
//! closed channel, every [`WriteCmd::Synthesized`] queued during the output
//! loop is delivered before container stdin closes.
//!
//! ## Failure tolerance
//!
//! - When container stdin returns `BrokenPipe` (the agent has already
//!   exited), the sink downgrades the failure to a single `warn!` and
//!   continues to drain the channel until every sender has been dropped. The
//!   exit-code path remains intact.
//! - When [`super::acp_policy::build_method_blocked_error`] fails (a
//!   theoretical edge case for a finite owned [`Value`]), the adapter logs
//!   a `warn!` and continues without sending a synthesized response,
//!   leaving the agent to time out rather than panicking the proxy.

use std::io;
use std::pin::Pin;

use ortho_config::serde_json::{self, Value};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

use super::acp_frame::{DeniedFrameDecision, FallbackReason, FrameOutput, OutboundFrameAssembler};
use super::acp_policy::build_method_blocked_error;

/// Bounded capacity for the container-stdin command channel.
///
/// Sized small so an agent flooding blocked methods cannot exhaust memory
/// before the sink drains them, while still permitting a few synthesized
/// responses to be queued ahead of the host-stdin forwarder during a
/// burst.
pub(super) const SINK_CHANNEL_CAPACITY: usize = 16;

/// One write destined for container stdin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WriteCmd {
    /// Bytes forwarded from host stdin (the operator's keystrokes or the
    /// hosting client's protocol output).
    Forward(Vec<u8>),
    /// A JSON-RPC error response synthesized by the policy adapter in
    /// response to a blocked request.
    Synthesized(Vec<u8>),
}

/// Drain the supplied channel, writing each command to `input` and flushing
/// after every successful write.
///
/// The function tolerates `BrokenPipe` from container stdin by logging a
/// single `warn!` and continuing to drain the channel until every sender
/// has been dropped. Once the channel is closed, the sink stops draining,
/// preserving the existing exit-code reporting path.
pub(super) async fn run_container_stdin_sink(
    mut input: Pin<Box<dyn AsyncWrite + Send>>,
    mut commands: mpsc::Receiver<WriteCmd>,
) -> io::Result<()> {
    let mut input_alive = true;

    while let Some(command) = commands.recv().await {
        let bytes = command_bytes(command);
        if input_alive {
            input_alive = write_command_bytes(&mut input, &bytes).await?;
        }
    }

    finalize_sink_writer(&mut input, input_alive).await;
    Ok(())
}

/// Consume either command variant without altering its payload bytes.
/// For example, `WriteCmd::Synthesized(bytes)` yields the same owned `bytes`.
fn command_bytes(command: WriteCmd) -> Vec<u8> {
    match command {
        WriteCmd::Forward(bytes) | WriteCmd::Synthesized(bytes) => bytes,
    }
}

/// Shut down a live sink after the command channel has drained.
/// For example, a sink already closed by `BrokenPipe` skips shutdown.
async fn finalize_sink_writer(input: &mut Pin<Box<dyn AsyncWrite + Send>>, input_alive: bool) {
    if !input_alive {
        return;
    }
    let outcome = input.shutdown().await;
    log_shutdown_outcome(outcome);
}

/// Report shutdown failure without replacing the session exit-code result.
/// For example, `Ok(())` emits no warning.
fn log_shutdown_outcome(outcome: io::Result<()>) {
    if let Err(error) = outcome {
        tracing::warn!(%error, "container stdin shutdown failed");
    }
}

/// Write and flush one command, reporting whether stdin remains usable.
/// For example, a broken pipe yields `Ok(false)` so later commands are drained.
async fn write_command_bytes(
    input: &mut Pin<Box<dyn AsyncWrite + Send>>,
    bytes: &[u8],
) -> io::Result<bool> {
    if !classify_pipe_outcome(input.write_all(bytes).await)? {
        return Ok(false);
    }
    classify_pipe_outcome(input.flush().await)
}

/// Treat a broken pipe as closed stdin while preserving other I/O errors.
/// For example, a successful write yields `Ok(true)`.
fn classify_pipe_outcome(result: io::Result<()>) -> io::Result<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
            report_broken_pipe(&error);
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

/// Warn once when the sink stops writing after the agent closes stdin.
/// For example, the caller logs the first broken pipe before draining commands.
fn report_broken_pipe(error: &io::Error) {
    tracing::warn!(%error, "container stdin closed; subsequent writes dropped");
}

/// Adapter that turns assembler outputs into host-stdout writes and
/// container-stdin commands.
pub(super) struct OutboundPolicyAdapter {
    /// Per-session framer that classifies complete agent-output frames.
    assembler: OutboundFrameAssembler,
    /// Bounded queue shared with the host-input forwarder for ordered stdin
    /// writes.
    sender: mpsc::Sender<WriteCmd>,
    /// Container identifier attached to policy diagnostics for correlation.
    container_id: String,
    /// Whether this adapter has already emitted its single fallback warning.
    fallback_logged: bool,
}

impl OutboundPolicyAdapter {
    /// Construct an adapter over the supplied assembler and sink sender.
    pub(super) fn new(
        assembler: OutboundFrameAssembler,
        sender: mpsc::Sender<WriteCmd>,
        container_id: impl Into<String>,
    ) -> Self {
        Self {
            assembler,
            sender,
            container_id: container_id.into(),
            fallback_logged: false,
        }
    }

    /// Process one Bollard output chunk, writing permitted bytes to
    /// `host_stdout` and queuing synthesized error responses on the sink
    /// channel.
    pub(super) async fn handle_chunk<W>(
        &mut self,
        chunk: &[u8],
        host_stdout: &mut W,
    ) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let (outputs, fallback) = self.assembler.ingest_chunk(chunk);
        for output in outputs {
            self.dispatch_output(output, host_stdout).await?;
        }
        if let Some(reason) = fallback {
            self.log_fallback_once(reason);
        }
        Ok(())
    }

    /// Finalize the adapter at end of stream, logging any partial-frame
    /// drop reported by the assembler.
    pub(super) fn finish(&mut self) {
        if let Some(reason) = self.assembler.finish() {
            self.log_fallback_once(reason);
        }
    }

    /// Write allowed output verbatim or handle a denial without exposing its
    /// frame.
    /// For example, a blocked notification produces no host-stdout bytes.
    async fn dispatch_output<W>(
        &mut self,
        output: FrameOutput,
        host_stdout: &mut W,
    ) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        match output {
            FrameOutput::Forward(bytes) => {
                host_stdout.write_all(&bytes).await?;
                host_stdout.flush().await
            }
            FrameOutput::Decision(decision, line_ending) => {
                self.handle_decision(decision, &line_ending).await;
                Ok(())
            }
        }
    }

    /// Log a denial and queue an error only for requests with response
    /// identifiers.
    /// For example, a blocked notification is logged and dropped without a
    /// reply.
    async fn handle_decision(&self, decision: DeniedFrameDecision, line_ending: &[u8]) {
        match decision {
            DeniedFrameDecision::BlockNotification { method } => {
                self.log_denial(&method, &Value::Null, "ACP blocked notification dropped");
            }
            DeniedFrameDecision::BlockRequest { id, method } => {
                self.log_denial(&method, &id, "ACP blocked request denied");
                self.queue_synthesized_error(&id, &method, line_ending)
                    .await;
            }
        }
    }

    /// Serialize a method-blocked response with the original frame line ending.
    /// For example, a CRLF request receives an error ending in CRLF.
    async fn queue_synthesized_error(&self, id: &Value, method: &str, line_ending: &[u8]) {
        match build_method_blocked_error(id, method, line_ending) {
            Ok(bytes) => self.send_synthesized_or_log(bytes, method).await,
            Err(error) => self.log_synthesis_failure(method, &error),
        }
    }

    /// Queue denial bytes through the single stdin owner, logging a closed
    /// sink.
    /// For example, a disconnected receiver causes a warning rather than a
    /// panic.
    async fn send_synthesized_or_log(&self, bytes: Vec<u8>, method: &str) {
        let outcome = self.sender.send(WriteCmd::Synthesized(bytes)).await;
        if let Err(error) = outcome {
            self.log_send_failure(method, &error);
        }
    }

    /// Correlate a failed denial enqueue with the container and blocked method.
    /// For example, a closed sink records the send error for the affected
    /// method.
    fn log_send_failure(&self, method: &str, error: &mpsc::error::SendError<WriteCmd>) {
        tracing::warn!(
            target = "podbot::acp::policy",
            container_id = %self.container_id,
            method = %method,
            ?error,
            "ACP denial response could not be queued; sink already closed",
        );
    }

    /// Report serialization failure while leaving the agent to time out.
    /// For example, a failed denial response records the method and JSON error.
    fn log_synthesis_failure(&self, method: &str, error: &serde_json::Error) {
        tracing::warn!(
            target = "podbot::acp::policy",
            container_id = %self.container_id,
            method = %method,
            %error,
            "ACP denial response failed to serialize; agent will time out",
        );
    }

    /// Record the blocked method and request identifier without forwarding
    /// bytes.
    /// For example, a notification uses `Value::Null` as its diagnostic
    /// identifier.
    fn log_denial(&self, method: &str, id: &Value, message: &'static str) {
        tracing::warn!(
            target = "podbot::acp::policy",
            container_id = %self.container_id,
            method = %method,
            id = %id,
            "{message}",
        );
    }

    /// Emit at most one fallback diagnostic during the adapter lifetime.
    /// For example, a second fallback reason is ignored after the first
    /// warning.
    fn log_fallback_once(&mut self, reason: FallbackReason) {
        if self.fallback_logged {
            return;
        }
        self.fallback_logged = true;
        self.emit_fallback_warning(reason);
    }

    /// Select the warning matching the framing fallback reason.
    /// For example, a dropped partial frame records its discarded byte count.
    fn emit_fallback_warning(&self, reason: FallbackReason) {
        match reason {
            FallbackReason::BufferOverflow => self.warn_buffer_overflow(),
            FallbackReason::DroppedPartialFrame { byte_count } => {
                self.warn_partial_frame_drop(byte_count);
            }
        }
    }

    /// Warn that overflow has switched the rest of the session to raw
    /// forwarding.
    /// For example, the warning identifies the container whose frame exceeded
    /// the limit.
    fn warn_buffer_overflow(&self) {
        tracing::warn!(
            target = "podbot::acp::policy",
            container_id = %self.container_id,
            "ACP runtime buffer overflowed; remaining bytes forwarded raw",
        );
    }

    /// Warn about unclassified bytes discarded at the end of the output stream.
    /// For example, `byte_count = 5` records five dropped bytes without their
    /// contents.
    fn warn_partial_frame_drop(&self, byte_count: usize) {
        tracing::warn!(
            target = "podbot::acp::policy",
            container_id = %self.container_id,
            byte_count,
            "ACP runtime dropped unauthorized partial frame at end of stream",
        );
    }
}

#[cfg(test)]
#[path = "acp_runtime_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "acp_runtime_bdd_tests.rs"]
mod bdd_tests;
