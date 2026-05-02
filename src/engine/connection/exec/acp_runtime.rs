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
//! processes commands sequentially before the closed-channel terminator
//! arrives, every [`WriteCmd::Synthesised`] queued during the output loop
//! is delivered before container stdin closes.
//!
//! ## Failure tolerance
//!
//! - When container stdin returns `BrokenPipe` (the agent has already
//!   exited), the sink downgrades the failure to a single `warn!` and
//!   continues to drain the channel until [`WriteCmd::Shutdown`]. The exit
//!   code path remains intact.
//! - When [`super::acp_policy::build_method_blocked_error`] fails (a
//!   theoretical edge case for a finite owned [`Value`]), the adapter logs
//!   a `warn!` and continues without sending a synthesized response,
//!   leaving the agent to time out rather than panicking the proxy.

use std::io;
use std::pin::Pin;

use ortho_config::serde_json::Value;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

use super::acp_frame::{FallbackReason, FrameOutput, OutboundFrameAssembler};
use super::acp_policy::{FrameDecision, build_method_blocked_error};

/// Bounded capacity for the container-stdin command channel.
///
/// Sized small so an agent flooding blocked methods cannot exhaust memory
/// before the sink drains them, while still permitting a few synthesized
/// responses to be queued ahead of the host-stdin forwarder during a
/// burst.
pub(super) const SINK_CHANNEL_CAPACITY: usize = 16;

/// One write destined for container stdin.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum WriteCmd {
    /// Bytes forwarded from host stdin (the operator's keystrokes or the
    /// hosting client's protocol output).
    Forward(Vec<u8>),
    /// A JSON-RPC error response synthesized by the policy adapter in
    /// response to a blocked request.
    Synthesised(Vec<u8>),
}

/// Drain the supplied channel, writing each command to `input` and flushing
/// after every successful write.
///
/// The function tolerates `BrokenPipe` from container stdin by logging a
/// single `warn!` and continuing to drain the channel until every sender
/// has been dropped, preserving the existing exit-code reporting path.
pub(super) async fn run_container_stdin_sink(
    mut input: Pin<Box<dyn AsyncWrite + Send>>,
    mut commands: mpsc::Receiver<WriteCmd>,
) -> io::Result<()> {
    let mut input_alive = true;

    while let Some(command) = commands.recv().await {
        let (WriteCmd::Forward(bytes) | WriteCmd::Synthesised(bytes)) = command;
        if input_alive {
            input_alive = write_command_bytes(&mut input, &bytes).await?;
        }
    }

    if input_alive {
        if let Err(error) = input.shutdown().await {
            tracing::warn!(%error, "container stdin shutdown failed");
        }
    }

    Ok(())
}

async fn write_command_bytes(
    input: &mut Pin<Box<dyn AsyncWrite + Send>>,
    bytes: &[u8],
) -> io::Result<bool> {
    match input.write_all(bytes).await {
        Ok(()) => match input.flush().await {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
                report_broken_pipe(error);
                Ok(false)
            }
            Err(error) => Err(error),
        },
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
            report_broken_pipe(error);
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn report_broken_pipe(error: io::Error) {
    tracing::warn!(%error, "container stdin closed; subsequent writes dropped");
}

/// Adapter that turns assembler outputs into host-stdout writes and
/// container-stdin commands.
pub(super) struct OutboundPolicyAdapter {
    assembler: OutboundFrameAssembler,
    sender: mpsc::Sender<WriteCmd>,
    container_id: String,
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

    async fn handle_decision(&self, decision: FrameDecision, line_ending: &[u8]) {
        match decision {
            FrameDecision::Forward => {}
            FrameDecision::BlockNotification { method } => {
                self.log_denial(&method, &Value::Null, "ACP blocked notification dropped");
            }
            FrameDecision::BlockRequest { id, method } => {
                self.log_denial(&method, &id, "ACP blocked request denied");
                self.queue_synthesized_error(&id, &method, line_ending).await;
            }
        }
    }

    async fn queue_synthesized_error(&self, id: &Value, method: &str, line_ending: &[u8]) {
        match build_method_blocked_error(id, method, line_ending) {
            Ok(bytes) => {
                if let Err(error) = self.sender.send(WriteCmd::Synthesised(bytes)).await {
                    tracing::warn!(
                        target = "podbot::acp::policy",
                        container_id = %self.container_id,
                        method = %method,
                        ?error,
                        "ACP denial response could not be queued; sink already closed",
                    );
                }
            }
            Err(error) => {
                tracing::warn!(
                    target = "podbot::acp::policy",
                    container_id = %self.container_id,
                    method = %method,
                    %error,
                    "ACP denial response failed to serialize; agent will time out",
                );
            }
        }
    }

    fn log_denial(&self, method: &str, id: &Value, message: &'static str) {
        tracing::warn!(
            target = "podbot::acp::policy",
            container_id = %self.container_id,
            method = %method,
            id = %id,
            "{message}",
        );
    }

    fn log_fallback_once(&mut self, reason: FallbackReason) {
        if self.fallback_logged {
            return;
        }
        self.fallback_logged = true;
        match reason {
            FallbackReason::BufferOverflow => {
                tracing::warn!(
                    target = "podbot::acp::policy",
                    container_id = %self.container_id,
                    "ACP runtime buffer overflowed; remaining bytes forwarded raw",
                );
            }
            FallbackReason::DroppedPartialFrame { byte_count } => {
                tracing::warn!(
                    target = "podbot::acp::policy",
                    container_id = %self.container_id,
                    byte_count,
                    "ACP runtime dropped unauthorized partial frame at end of stream",
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "acp_runtime_tests.rs"]
mod tests;
