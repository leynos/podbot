//! Composed cloning cleanup regressions use buffered receipts and no engine I/O.

use std::cell::RefCell;
use std::io::{self, Write};

use rstest::rstest;
use tokio::runtime::Runtime;

use super::tests::{FailingReceiptWriter, ReceiptFailure, empty_bundle};
use super::{
    BollardError, ContainerInspectResponse, StepResult, finish_and_record,
    record_container_receipt, remove_and_verify,
};

/// Finite outcomes at the removal and inspection boundaries.
#[derive(Clone, Copy)]
enum CleanupOutcome {
    /// The owned ID is absent after removal.
    Absent,
    /// Removal fails before inspection can run.
    RemovalFailure,
    /// The removed ID remains inspectable.
    Present,
    /// Inspection returns another present identity rather than absence.
    OtherContainerPresent,
    /// Inspection refuses authentication.
    Unauthorized,
    /// Inspection refuses authorization.
    Forbidden,
    /// The server cannot complete inspection.
    ServerFailure,
    /// Inspection cannot reach the server.
    TransportFailure,
}

/// Observable ordering includes the actual ID passed to each operation.
#[derive(Debug, PartialEq, Eq)]
enum CleanupEvent {
    /// Removal started for this independent identity.
    Remove(String),
    /// Inspection received this requested identity.
    Inspect(String),
    /// Receipt emission received this verified identity.
    Record(String),
}

/// Supply a controlled inspect response without connecting to an engine.
fn inspection_response(outcome: CleanupOutcome) -> Result<ContainerInspectResponse, BollardError> {
    let status_code = match outcome {
        CleanupOutcome::Present => return Ok(ContainerInspectResponse::default()),
        CleanupOutcome::OtherContainerPresent => {
            return Ok(ContainerInspectResponse {
                id: Some(String::from("another-container")),
                ..ContainerInspectResponse::default()
            });
        }
        CleanupOutcome::TransportFailure => {
            return Err(BollardError::IOError {
                err: io::Error::new(io::ErrorKind::ConnectionRefused, "engine unreachable"),
            });
        }
        CleanupOutcome::Unauthorized => 401,
        CleanupOutcome::Forbidden => 403,
        CleanupOutcome::ServerFailure => 500,
        CleanupOutcome::Absent | CleanupOutcome::RemovalFailure => 404,
    };
    Err(BollardError::DockerResponseServerError {
        status_code,
        message: String::from("controlled engine response"),
    })
}

/// Run the production-used composition with controlled operations and output.
///
/// Only this regression module uses these callbacks. Assertions stay in test
/// bodies; synthetic IDs never reach the default stdout receipt adapter.
fn controlled_cleanup(
    runtime: &Runtime,
    container_id: &str,
    outcome: CleanupOutcome,
    writer: &mut impl Write,
) -> (StepResult<()>, Vec<CleanupEvent>) {
    let events = RefCell::new(Vec::new());
    let inspection_events = &events;
    let result = finish_and_record(
        container_id,
        || {
            runtime.block_on(remove_and_verify(
                container_id,
                async {
                    events
                        .borrow_mut()
                        .push(CleanupEvent::Remove(container_id.to_owned()));
                    if matches!(outcome, CleanupOutcome::RemovalFailure) {
                        Err(String::from("controlled removal failure"))
                    } else {
                        Ok(())
                    }
                },
                |requested_id| async move {
                    inspection_events
                        .borrow_mut()
                        .push(CleanupEvent::Inspect(requested_id.to_owned()));
                    inspection_response(outcome)
                },
            ))
        },
        |stage, recorded_id| {
            events
                .borrow_mut()
                .push(CleanupEvent::Record(recorded_id.to_owned()));
            record_container_receipt(writer, stage, recorded_id)
        },
    );
    (result, events.into_inner())
}

#[test]
fn successful_cleanup_records_each_stage_once_for_the_removed_id() {
    let bundle = empty_bundle().expect("inert runtime bundle must initialize");
    let mut output = Vec::new();
    bundle
        .record_created_to(&mut output)
        .expect("buffered creation must succeed");
    let (result, events) = controlled_cleanup(
        &bundle.runtime,
        &bundle.container_id,
        CleanupOutcome::Absent,
        &mut output,
    );
    assert_eq!(result, Ok(()), "404 after removal completes cleanup");
    assert_eq!(
        events,
        vec![
            CleanupEvent::Remove(bundle.container_id.clone()),
            CleanupEvent::Inspect(bundle.container_id.clone()),
            CleanupEvent::Record(bundle.container_id.clone()),
        ],
        "inspection and receipt must use the independently recorded removed ID"
    );
    let records = output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(serde_json::from_slice::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()
        .expect("both buffered lifecycle records must be valid JSON");
    let expected = ["created", "removed"].map(|stage| {
        serde_json::json!({
            "marker": "podbot.e2e.owned-container", "schema": 1, "stage": stage,
            "container_id": bundle.container_id, "owner_pid": std::process::id(),
        })
    });
    assert_eq!(
        records, expected,
        "exactly one five-key record per stage is required"
    );
    assert_eq!(
        output.last(),
        Some(&b'\n'),
        "the last record must be terminated"
    );
}

#[rstest]
#[case::removal(CleanupOutcome::RemovalFailure, "controlled removal failure", false)]
#[case::present(CleanupOutcome::Present, "still exists", true)]
#[case::other_id_present(CleanupOutcome::OtherContainerPresent, "still exists", true)]
#[case::unauthorized(CleanupOutcome::Unauthorized, "cannot verify removal", true)]
#[case::forbidden(CleanupOutcome::Forbidden, "cannot verify removal", true)]
#[case::server_failure(CleanupOutcome::ServerFailure, "cannot verify removal", true)]
#[case::transport(CleanupOutcome::TransportFailure, "engine unreachable", true)]
fn cleanup_failures_prevent_removed_receipts(
    #[case] outcome: CleanupOutcome,
    #[case] diagnostic: &str,
    #[case] should_inspect: bool,
) {
    let bundle = empty_bundle().expect("inert runtime bundle must initialize");
    let mut output = Vec::new();
    let (result, events) =
        controlled_cleanup(&bundle.runtime, &bundle.container_id, outcome, &mut output);
    let error = result.expect_err("failed cleanup must fail its lifecycle caller");
    assert!(
        error.contains(diagnostic),
        "the actual failure cause must propagate: {error}"
    );
    let mut expected = vec![CleanupEvent::Remove(bundle.container_id.clone())];
    if should_inspect {
        expected.push(CleanupEvent::Inspect(bundle.container_id.clone()));
    }
    assert_eq!(
        events, expected,
        "a failed boundary must short-circuit later operations"
    );
    assert!(
        output.is_empty(),
        "failed removal or observation cannot emit a removed receipt"
    );
}

#[rstest]
#[case::write(ReceiptFailure::Write, "receipt write failed")]
#[case::flush(ReceiptFailure::Flush, "receipt flush failed")]
fn verified_cleanup_propagates_receipt_output_failure(
    #[case] failure: ReceiptFailure,
    #[case] diagnostic: &str,
) {
    let bundle = empty_bundle().expect("inert runtime bundle must initialize");
    let (result, events) = controlled_cleanup(
        &bundle.runtime,
        &bundle.container_id,
        CleanupOutcome::Absent,
        &mut FailingReceiptWriter { failure },
    );
    let error = result.expect_err("receipt output failure must fail the cleanup caller");
    assert!(
        error.contains(diagnostic),
        "the failing output stage must propagate"
    );
    assert!(
        error.contains(&bundle.container_id),
        "the output error must retain the owned ID"
    );
    assert_eq!(
        events,
        vec![
            CleanupEvent::Remove(bundle.container_id.clone()),
            CleanupEvent::Inspect(bundle.container_id.clone()),
            CleanupEvent::Record(bundle.container_id.clone()),
        ],
        "receipt output is attempted only after removal and same-ID verification"
    );
}
