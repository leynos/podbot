//! Exhaustive chunk-split reassembly tests for the streaming Agentic Control
//! Protocol (ACP) frame assembler.
//!
//! These tests build a deterministic multi-frame byte stream and verify that
//! every two-way split, and a broad selection of three-way splits, reassemble
//! byte-identically through [`OutboundFrameAssembler`].

use ortho_config::serde_json;
use rstest::rstest;

use super::{assembler, collect_forward_bytes, permitted_frame};

/// Build a deterministic byte sequence containing several permitted ACP
/// frames. The sequence is reused across the exhaustive-split parameterized
/// tests below.
fn permitted_stream() -> Result<Vec<u8>, serde_json::Error> {
    let frames = [
        permitted_frame("session/new", b"\n")?,
        permitted_frame("session/update", b"\n")?,
        permitted_frame("session/cancel", b"\n")?,
        permitted_frame("session/new", b"\r\n")?,
        permitted_frame("session/update", b"\n")?,
    ];
    Ok(frames.into_iter().fold(Vec::new(), |mut acc, mut frame| {
        acc.append(&mut frame);
        acc
    }))
}

fn assemble_with_two_chunks(stream: &[u8], split_at: usize) -> Vec<u8> {
    let mut framer = assembler();
    let first = stream.get(..split_at).unwrap_or_default();
    let second = stream.get(split_at..).unwrap_or_default();
    let mut outputs = Vec::new();
    let (chunk_one, fallback_one) = framer.ingest_chunk(first);
    assert!(fallback_one.is_none());
    outputs.extend(chunk_one);
    let (chunk_two, fallback_two) = framer.ingest_chunk(second);
    assert!(fallback_two.is_none());
    outputs.extend(chunk_two);
    assert!(framer.finish().is_none());
    collect_forward_bytes(&outputs)
}

fn assemble_with_three_chunks(stream: &[u8], first_split: usize, second_split: usize) -> Vec<u8> {
    assert!(first_split <= second_split);
    let mut framer = assembler();
    let first = stream.get(..first_split).unwrap_or_default();
    let second = stream.get(first_split..second_split).unwrap_or_default();
    let third = stream.get(second_split..).unwrap_or_default();
    let mut outputs = Vec::new();
    for chunk in [first, second, third] {
        let (chunk_outputs, fallback) = framer.ingest_chunk(chunk);
        assert!(fallback.is_none());
        outputs.extend(chunk_outputs);
    }
    assert!(framer.finish().is_none());
    collect_forward_bytes(&outputs)
}

#[test]
fn every_two_way_split_reassembles_to_original_byte_stream() {
    let stream = permitted_stream().expect("stream should serialize");
    for split_at in 1..stream.len() {
        let reassembled = assemble_with_two_chunks(&stream, split_at);
        assert_eq!(
            reassembled, stream,
            "split at byte {split_at} should reassemble byte-identically",
        );
    }
}

#[rstest]
#[case(1, 4)]
#[case(8, 32)]
#[case(16, 64)]
#[case(32, 96)]
#[case(40, 80)]
#[case(50, 120)]
#[case(60, 100)]
#[case(70, 140)]
#[case(80, 160)]
#[case(90, 150)]
#[case(95, 145)]
#[case(100, 200)]
#[case(110, 220)]
#[case(120, 180)]
#[case(125, 230)]
#[case(130, 240)]
#[case(135, 235)]
#[case(140, 250)]
#[case(150, 260)]
#[case(155, 265)]
#[case(160, 270)]
#[case(170, 280)]
#[case(180, 290)]
#[case(190, 300)]
#[case(200, 310)]
#[case(210, 320)]
#[case(220, 325)]
#[case(225, 330)]
#[case(230, 335)]
#[case(235, 340)]
#[case(240, 345)]
#[case(250, 350)]
fn three_way_splits_reassemble_to_original_byte_stream(
    #[case] first_split: usize,
    #[case] second_split: usize,
) {
    let stream = permitted_stream().expect("stream should serialize");
    let first_clamped = first_split.min(stream.len());
    let second_clamped = second_split.min(stream.len());
    let reassembled = assemble_with_three_chunks(&stream, first_clamped, second_clamped);
    assert_eq!(
        reassembled, stream,
        "three-way split at ({first_clamped}, {second_clamped}) should reassemble identically",
    );
}
