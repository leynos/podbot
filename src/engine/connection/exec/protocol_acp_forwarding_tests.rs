//! ACP stdin forwarding tests for the protocol proxy.

use super::*;

#[test]
fn forwarding_leaves_initialize_unchanged_when_acp_rewrite_is_disabled() {
    let host_stdin_bytes = initialize_frame("\n");

    let (forwarded, shutdown_called) = run_forwarding_with_rewrite(&host_stdin_bytes, false);

    assert_eq!(
        forwarded, host_stdin_bytes,
        "generic protocol sessions should retain raw byte-stream semantics"
    );
    assert!(shutdown_called, "stdin forwarding should shut down input");
}

#[test]
fn forwarding_masks_initialize_and_preserves_trailing_bytes() {
    let mut host_stdin_bytes = initialize_frame("\n");
    let trailing = initialize_frame("\n");
    host_stdin_bytes.extend_from_slice(&trailing);

    let (forwarded, shutdown_called) = run_forwarding(&host_stdin_bytes);
    let newline_index = forwarded
        .iter()
        .position(|byte| *byte == b'\n')
        .expect("masked initialize should remain line terminated");
    let initialize_frame = forwarded
        .get(..=newline_index)
        .expect("masked initialize frame should remain addressable");
    let trailing_forwarded = forwarded
        .get(newline_index + 1..)
        .expect("trailing bytes should remain addressable");
    let payload = parse_frame_payload(initialize_frame);

    assert_masked_client_capabilities(&payload);
    assert_eq!(
        trailing_forwarded,
        trailing.as_slice(),
        "trailing bytes should pass through unchanged"
    );
    assert!(shutdown_called, "stdin forwarding should shut down input");
}

#[test]
fn forwarding_does_not_wait_indefinitely_for_oversized_initial_frame() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime should build");
    let test_timeout = std::time::Duration::from_secs(1);
    let host_stdin_bytes = vec![b'x'; MAX_FIRST_FRAME_BYTES + 1];
    let (host_writer, host_reader) = runtime
        .block_on(async {
            let (mut host_writer, host_reader) = tokio::io::duplex(host_stdin_bytes.len());
            host_writer.write_all(&host_stdin_bytes).await?;
            io::Result::Ok((host_writer, host_reader))
        })
        .expect("host stdin should accept oversized initial bytes");

    let mut buffered_stdin =
        tokio::io::BufReader::with_capacity(STDIN_BUFFER_CAPACITY, host_reader);
    let recording_input = RecordingInputWriter::new();
    let forwarded_bytes = recording_input.bytes.clone();
    let mut container_input: Pin<Box<dyn AsyncWrite + Send>> = Box::pin(recording_input);

    runtime
        .block_on(async {
            tokio::time::timeout(
                test_timeout,
                forward_initial_acp_frame_async(&mut buffered_stdin, &mut container_input),
            )
            .await
        })
        .expect("initial forwarding should not wait for newline or EOF")
        .expect("initial forwarding should succeed");

    assert_eq!(
        forwarded_bytes
            .lock()
            .expect("writer mutex should not poison")
            .len(),
        MAX_FIRST_FRAME_BYTES,
        "only the bounded first-frame buffer should be held before streaming resumes"
    );

    drop(host_writer);
}
