//! Cleanup verdict regressions distinguish absence from failed observation.

use super::{
    BollardError, ContainerInspectResponse, ContainerReceiptStage, require_container_absence,
    write_container_receipt,
};
use rstest::rstest;
use std::io::{self, Write};

#[rstest]
#[case::absent(404, true)]
#[case::unauthorized(401, false)]
#[case::forbidden(403, false)]
#[case::server_failure(500, false)]
fn only_not_found_confirms_removal(#[case] status_code: u16, #[case] expected: bool) {
    let inspection = Err(BollardError::DockerResponseServerError {
        status_code,
        message: String::from("engine response"),
    });
    let result = require_container_absence("owned-id", &inspection);
    assert_eq!(result.is_ok(), expected, "only server 404 proves absence");
    if !expected {
        let error = result.expect_err("observation failure must remain an error");
        assert!(
            error.contains("owned-id"),
            "cleanup errors must identify the owned container"
        );
    }
}

#[test]
fn a_present_container_is_not_removed() {
    let inspection = Ok(ContainerInspectResponse::default());
    let error = require_container_absence("owned-id", &inspection)
        .expect_err("an inspectable container is still present");
    assert!(
        error.contains("owned-id") && error.contains("still exists"),
        "presence must fail with the owned container identity"
    );
}

#[test]
fn a_transport_failure_is_not_absence() {
    let inspection = Err(BollardError::IOError {
        err: std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "engine unreachable"),
    });
    let error = require_container_absence("owned-id", &inspection)
        .expect_err("an unavailable engine cannot prove absence");
    assert!(
        error.contains("owned-id") && error.contains("engine unreachable"),
        "transport diagnostics must survive cleanup verification"
    );
}

#[rstest]
#[case::created(ContainerReceiptStage::Created, "created")]
#[case::removed(ContainerReceiptStage::Removed, "removed")]
fn receipt_is_one_versioned_record_with_exact_identity(
    #[case] stage: ContainerReceiptStage,
    #[case] expected_stage: &str,
) {
    let id = "owned-\"line\ncontainer";
    let mut output = Vec::new();
    write_container_receipt(&mut output, stage, id, 42)
        .expect("in-memory receipt should be written");
    assert!(
        output
            .split_last()
            .is_some_and(|(last, body)| *last == b'\n' && !body.contains(&b'\n')),
        "an escaped ID must not introduce extra record boundaries"
    );
    assert_eq!(
        output.last(),
        Some(&b'\n'),
        "receipt must end at a line boundary"
    );
    let record: serde_json::Value =
        serde_json::from_slice(&output).expect("receipt must be valid JSON");
    assert_eq!(
        record,
        serde_json::json!({
            "marker": "podbot.e2e.owned-container", "schema": 1,
            "stage": expected_stage, "container_id": id, "owner_pid": 42,
        }),
        "the external test-result schema and exact identity must be preserved"
    );
}

/// Distinct output failures that must fail the receipt's owning test.
#[derive(Clone, Copy)]
pub(super) enum ReceiptFailure {
    Write,
    Flush,
}

/// Inject an output failure without changing process stdout or engine state.
pub(super) struct FailingReceiptWriter {
    pub(super) failure: ReceiptFailure,
}

impl Write for FailingReceiptWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self.failure {
            ReceiptFailure::Write => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "receipt write failed",
            )),
            ReceiptFailure::Flush => Ok(bytes.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.failure {
            ReceiptFailure::Write => Ok(()),
            ReceiptFailure::Flush => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "receipt flush failed",
            )),
        }
    }
}

#[rstest]
#[case::write(ReceiptFailure::Write, "receipt write failed")]
#[case::flush(ReceiptFailure::Flush, "receipt flush failed")]
fn receipt_output_errors_propagate(
    #[case] failure: ReceiptFailure,
    #[case] expected_message: &str,
) {
    let mut writer = FailingReceiptWriter { failure };
    let error =
        write_container_receipt(&mut writer, ContainerReceiptStage::Created, "owned-id", 42)
            .expect_err("a failed receipt output must fail its caller");
    assert_eq!(
        error.kind(),
        io::ErrorKind::BrokenPipe,
        "preserve the output error kind"
    );
    assert_eq!(
        error.to_string(),
        expected_message,
        "preserve the failing output stage"
    );
}

/// Construct an inert bundle; no network request or lifecycle record is emitted.
///
/// Only ownership and created-output regressions use this fixture. An absent
/// container makes fallback drop inert; the configured client is never called.
#[rstest::fixture]
pub(super) fn empty_bundle() -> super::StepResult<super::SandboxBundle> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    let docker =
        bollard::Docker::connect_with_http("http://127.0.0.1:9", 1, bollard::API_DEFAULT_VERSION)
            .map_err(|error| error.to_string())?;
    Ok(super::SandboxBundle {
        runtime: std::sync::Arc::new(runtime),
        container: None,
        docker: std::sync::Arc::new(docker),
        container_id: String::from("buffer-only-owned-id"),
    })
}

#[test]
fn missing_state_bundle_fails_before_cleanup() {
    let state = super::RepositoryCloningE2eState::default();
    let error = state
        .finish()
        .expect_err("missing ownership must fail cleanup");
    assert_eq!(error, "sandbox bundle missing at scenario cleanup");
    assert!(state.bundle.is_empty(), "cleanup cannot create ownership");
}

#[rstest]
fn shared_state_bundle_preserves_the_retained_owner(
    empty_bundle: super::StepResult<super::SandboxBundle>,
) {
    let bundle = std::sync::Arc::new(empty_bundle.expect("inert bundle must initialize"));
    let state = super::RepositoryCloningE2eState::default();
    state.bundle.set(std::sync::Arc::clone(&bundle));
    let error = state
        .finish()
        .expect_err("retained ownership must prevent cleanup");
    assert_eq!(error, "sandbox bundle still shared at scenario cleanup");
    assert_eq!(
        std::sync::Arc::strong_count(&bundle),
        1,
        "external owner survives rejection"
    );
    assert!(
        state.bundle.is_empty(),
        "the state must release its rejected reference"
    );
    assert!(
        bundle.container.is_none(),
        "inert setup must never acquire a container"
    );
}

#[rstest]
#[case::write(ReceiptFailure::Write, "receipt write failed")]
#[case::flush(ReceiptFailure::Flush, "receipt flush failed")]
fn created_caller_propagates_output_failure(
    empty_bundle: super::StepResult<super::SandboxBundle>,
    #[case] failure: ReceiptFailure,
    #[case] expected_message: &str,
) {
    let bundle = empty_bundle.expect("inert bundle must initialize");
    let error = bundle
        .record_created_to(&mut FailingReceiptWriter { failure })
        .expect_err("creation must propagate its receipt failure");
    assert!(
        error.contains(&bundle.container_id),
        "caller error must identify ownership"
    );
    assert!(
        error.contains(expected_message),
        "caller must retain the output cause"
    );
}
