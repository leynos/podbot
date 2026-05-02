//! Newline-delimited frame assembler for Agentic Control Protocol (ACP)
//! runtime enforcement.
//!
//! The Bollard exec stream delivers `LogOutput` chunks of arbitrary size; ACP
//! frames may begin and end at any byte offset within the stream. This module
//! buffers incoming bytes until a `\n` byte completes a frame, then asks the
//! pure policy in `acp_policy` to decide whether the frame should be
//! forwarded byte-identically, dropped silently, or replaced by a synthesized
//! error response.
//!
//! ## Design invariants
//!
//! - Permitted frames are forwarded **verbatim** (the original byte slice,
//!   including its line ending). The policy parses to decide; it never
//!   re-serializes. This preserves any agent-side integrity assumptions
//!   such as key ordering or embedded hashes.
//! - The buffer is bounded by [`MAX_RUNTIME_FRAME_BYTES`] (128 KiB). When
//!   the buffer fills before a newline is observed, the assembler flushes
//!   the buffered bytes verbatim, sets a one-shot raw-fallback flag, and
//!   forwards every subsequent chunk unchanged for the rest of the session.
//!   The fallback is logged exactly once by the adapter.
//! - At end of stream, any residual partial frame is **dropped**. A frame
//!   that has not been classified must never reach host stdout, since
//!   forwarding unauthorized bytes would re-introduce the leak this step
//!   is meant to prevent. The adapter logs the dropped byte count once.
//!
//! ## Concurrency
//!
//! Each protocol session owns one assembler on a single Tokio task. The
//! assembler holds no locks, no channels, and no `tokio` types; it is
//! purely synchronous data manipulation.

use super::acp_policy::{FrameDecision, MethodDenylist, evaluate_agent_outbound_frame};

/// Maximum bytes buffered while searching for a frame's terminating newline.
///
/// 128 kibibytes — twice the input ceiling, since agent-emitted ACP
/// `prompt`-style payloads can carry embedded resources that exceed the
/// host-driven input frames.
pub(crate) const MAX_RUNTIME_FRAME_BYTES: usize = 131_072;

/// Outcome for a single completed frame produced by
/// [`OutboundFrameAssembler`].
///
/// `Forward` carries the verbatim bytes that the adapter must write to host
/// stdout (including any original line ending). `Decision` carries the
/// policy verdict together with the original line-ending bytes that the
/// adapter should reuse when synthesizing an error response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FrameOutput {
    /// Forward these bytes to host stdout unchanged.
    Forward(Vec<u8>),
    /// Apply the policy decision; the trailing slice is the original line
    /// ending (`b""`, `b"\n"`, or `b"\r\n"`) for synthesized responses.
    Decision(FrameDecision, Vec<u8>),
}

/// Reason recorded once when the assembler enters raw-fallback mode at end
/// of stream or after the buffer overflows. The adapter logs exactly one
/// stderr record per session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FallbackReason {
    /// The buffer reached [`MAX_RUNTIME_FRAME_BYTES`] before a newline was
    /// observed. The assembler emitted the buffered bytes verbatim and
    /// forwards every subsequent chunk unchanged.
    BufferOverflow,
    /// End of stream was reached with bytes still buffered. The bytes are
    /// **dropped** — they have not been classified by the policy.
    DroppedPartialFrame {
        /// Number of buffered bytes that were dropped.
        byte_count: usize,
    },
}

/// Streaming assembler that splits ACP output into newline-delimited frames
/// and applies the supplied [`MethodDenylist`].
#[derive(Debug)]
pub(crate) struct OutboundFrameAssembler {
    buffer: Vec<u8>,
    denylist: MethodDenylist,
    raw_fallback: bool,
}

impl OutboundFrameAssembler {
    /// Construct a new assembler over the supplied denylist.
    pub(crate) fn new(denylist: MethodDenylist) -> Self {
        Self {
            buffer: Vec::with_capacity(8_192),
            denylist,
            raw_fallback: false,
        }
    }

    /// Process one chunk of agent-outbound bytes.
    ///
    /// Returns the sequence of [`FrameOutput`] decisions that the chunk
    /// produced, in order. When the assembler is in raw-fallback mode every
    /// chunk yields a single [`FrameOutput::Forward`] holding the chunk
    /// verbatim.
    pub(crate) fn ingest_chunk(&mut self, chunk: &[u8]) -> (Vec<FrameOutput>, Option<FallbackReason>) {
        if self.raw_fallback {
            return (
                if chunk.is_empty() {
                    Vec::new()
                } else {
                    vec![FrameOutput::Forward(chunk.to_vec())]
                },
                None,
            );
        }

        let mut outputs = Vec::new();
        let mut fallback = None;
        let mut cursor = 0;

        while cursor < chunk.len() {
            let remaining = chunk.get(cursor..).unwrap_or(&[]);
            if let Some(newline_offset) = remaining.iter().position(|byte| *byte == b'\n') {
                let frame_end = cursor + newline_offset + 1;
                let frame_slice = chunk.get(cursor..frame_end).unwrap_or(&[]);
                outputs.push(self.complete_frame(frame_slice));
                cursor = frame_end;
                continue;
            }

            // No newline in the rest of the chunk; append and stop.
            let pending = chunk.get(cursor..).unwrap_or(&[]);
            if self.buffer.len() + pending.len() > MAX_RUNTIME_FRAME_BYTES {
                outputs.push(self.flush_buffer_for_overflow(pending));
                fallback = Some(FallbackReason::BufferOverflow);
                self.raw_fallback = true;
                break;
            }
            self.buffer.extend_from_slice(pending);
            break;
        }

        (outputs, fallback)
    }

    fn complete_frame(&mut self, fresh_bytes: &[u8]) -> FrameOutput {
        if self.buffer.is_empty() {
            return classify_frame(fresh_bytes, &self.denylist);
        }
        self.buffer.extend_from_slice(fresh_bytes);
        let frame = std::mem::take(&mut self.buffer);
        classify_frame(&frame, &self.denylist)
    }

    fn flush_buffer_for_overflow(&mut self, pending: &[u8]) -> FrameOutput {
        let mut bytes = std::mem::take(&mut self.buffer);
        bytes.extend_from_slice(pending);
        FrameOutput::Forward(bytes)
    }

    /// Finalize the assembler at end of stream.
    ///
    /// If a partial frame remains buffered it is **dropped** and the byte
    /// count is reported via [`FallbackReason::DroppedPartialFrame`] so the
    /// adapter can log it exactly once. When the assembler is already in
    /// raw-fallback mode (overflow during the session), `finish` returns
    /// `None` because every byte has already been forwarded verbatim.
    pub(crate) fn finish(&mut self) -> Option<FallbackReason> {
        if self.raw_fallback || self.buffer.is_empty() {
            return None;
        }
        let byte_count = self.buffer.len();
        self.buffer.clear();
        Some(FallbackReason::DroppedPartialFrame { byte_count })
    }

    /// Return `true` when the assembler has fallen back to raw forwarding.
    #[cfg(test)]
    pub(crate) fn is_raw_fallback(&self) -> bool {
        self.raw_fallback
    }
}

fn classify_frame(frame_bytes: &[u8], denylist: &MethodDenylist) -> FrameOutput {
    let decision = evaluate_agent_outbound_frame(frame_bytes, denylist);
    match decision {
        FrameDecision::Forward => FrameOutput::Forward(frame_bytes.to_vec()),
        other => {
            let line_ending = trailing_line_ending(frame_bytes).to_vec();
            FrameOutput::Decision(other, line_ending)
        }
    }
}

fn trailing_line_ending(frame: &[u8]) -> &[u8] {
    if frame.ends_with(b"\r\n") {
        b"\r\n"
    } else if frame.ends_with(b"\n") {
        b"\n"
    } else {
        b""
    }
}

#[cfg(test)]
#[path = "acp_frame_tests.rs"]
mod tests;
