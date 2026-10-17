//! Shared state and fixture for the end-to-end repository-cloning scenarios.

use std::io::{self, Write};
use std::sync::Arc;

use bollard::Docker;
use bollard::errors::Error as BollardError;
use bollard::models::ContainerInspectResponse;
use podbot::engine::RepositoryCloneResult;
use podbot::error::PodbotError;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;
use testcontainers::ContainerAsync;
use testcontainers::GenericImage;
use tokio::runtime::Runtime;

/// Step result type for the end-to-end repository-cloning scenarios.
pub type StepResult<T> = Result<T, String>;

/// Owned bundle of the sandbox container, its accompanying tokio runtime, and
/// the Bollard client pointed at the same socket.
///
/// `ContainerAsync<I>` uses `tokio::runtime::Handle::current()` inside its
/// asynchronous drop helper. Dropping the container outside an active tokio
/// runtime context therefore panics with "there is no reactor running". The
/// custom [`Drop`] implementation below explicitly tears the container down
/// via `runtime.block_on(...)` so the inner runtime is still live while
/// `ContainerAsync` releases its resources.
pub struct SandboxBundle {
    /// Tokio runtime kept alive for the lifetime of the scenario.
    pub runtime: Arc<Runtime>,
    /// The sandbox container; taken during [`Drop`] to release synchronously.
    pub container: Option<ContainerAsync<GenericImage>>,
    /// Bollard client used to drive the production exec path.
    pub docker: Arc<Docker>,
    /// Container identifier surfaced to step definitions.
    pub container_id: String,
}

impl SandboxBundle {
    /// Remove this owned container and require the engine to confirm its absence.
    ///
    /// For example, `bundle.finish()` returns `Ok(())` only after removal and a
    /// server 404 for the same container ID; transport failures remain errors.
    /// Only completed cloning scenarios and the retained-client test call this.
    pub fn finish(mut self) -> StepResult<()> {
        let container = self
            .container
            .take()
            .ok_or_else(|| format!("owned container {} was already taken", self.container_id))?;
        self.runtime.block_on(async {
            container.rm().await.map_err(|error| {
                format!(
                    "failed to remove owned container {}: {error}",
                    self.container_id
                )
            })?;
            let inspection = self
                .docker
                .inspect_container(&self.container_id, None)
                .await;
            require_container_absence(&self.container_id, &inspection)
        })?;
        emit_container_receipt(ContainerReceiptStage::Removed, &self.container_id)
    }

    /// Emit ownership evidence only after this runtime-safe bundle exists.
    ///
    /// For example, `bundle.record_created()` writes one created record or
    /// returns an output error; the caller's bundle then retains RAII cleanup.
    pub(super) fn record_created(&self) -> StepResult<()> {
        emit_container_receipt(ContainerReceiptStage::Created, &self.container_id)
    }
}

/// Lifecycle stages exposed only by the cloning harness's test-data contract.
#[derive(Clone, Copy)]
enum ContainerReceiptStage {
    /// The fixture owns a started container and its runtime-safe cleanup bundle.
    Created,
    /// Explicit removal succeeded and inspection of the same ID returned 404.
    Removed,
}

/// Emit one structured test-result record to the existing captured stdout.
///
/// For example, a created record identifies its direct container ID and native
/// test PID for the gate runner. Output failures fail the enclosing test; no
/// logger or diagnostic-print policy is involved. Only bundle lifecycle uses it.
fn emit_container_receipt(stage: ContainerReceiptStage, container_id: &str) -> StepResult<()> {
    write_container_receipt(
        &mut io::stdout().lock(),
        stage,
        container_id,
        std::process::id(),
    )
    .map_err(|error| format!("failed to record owned container {container_id}: {error}"))
}

/// Write one schema-1 JSON line, preserving serialization, write and flush errors.
///
/// For example, writing the created stage for `owned-id` and PID 42 to a byte
/// vector produces exactly one JSON record ending in a newline. The private
/// writer seam serves only this receipt contract and its failure-path tests.
fn write_container_receipt(
    writer: &mut impl Write,
    stage: ContainerReceiptStage,
    container_id: &str,
    owner_pid: u32,
) -> io::Result<()> {
    let stage_name = match stage {
        ContainerReceiptStage::Created => "created",
        ContainerReceiptStage::Removed => "removed",
    };
    let mut record = serde_json::to_vec(&serde_json::json!({
        "marker": "podbot.e2e.owned-container",
        "schema": 1,
        "stage": stage_name,
        "container_id": container_id,
        "owner_pid": owner_pid,
    }))
    .map_err(io::Error::other)?;
    record.push(b'\n');
    writer.write_all(&record)?;
    writer.flush()
}

/// Accept only an engine 404 as proof that the exact owned container is absent.
///
/// For example, a permission failure for `owned-id` returns an error containing
/// that ID; it cannot be mistaken for successful removal. This query belongs
/// only to the cloning harness's explicit cleanup boundary.
fn require_container_absence(
    container_id: &str,
    inspection: &Result<ContainerInspectResponse, BollardError>,
) -> StepResult<()> {
    match inspection {
        Err(BollardError::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(()),
        Ok(_) => Err(format!(
            "owned container {container_id} still exists after removal"
        )),
        Err(error) => Err(format!(
            "cannot verify removal of owned container {container_id}: {error}"
        )),
    }
}

impl Drop for SandboxBundle {
    fn drop(&mut self) {
        let Some(container) = self.container.take() else {
            return;
        };
        // Run the container teardown inside an active runtime task so the
        // `async_drop` helper inside `ContainerAsync` can resolve
        // `Handle::current()` and call `block_in_place` from a worker context.
        self.runtime.block_on(async move {
            drop(container.rm().await);
        });
    }
}

/// Shared scenario state for the end-to-end repository-cloning BDD tests.
#[derive(Default, ScenarioState)]
pub struct RepositoryCloningE2eState {
    /// Sandbox container guard kept alive for the duration of the scenario.
    pub(crate) bundle: Slot<Arc<SandboxBundle>>,
    /// `GIT_ASKPASS` helper path inside the container.
    pub(crate) askpass_path: Slot<String>,
    /// Absolute workspace path inside the container.
    pub(crate) workspace_base_dir: Slot<String>,
    /// Outcome of the most recent clone attempt.
    pub(crate) outcome: Slot<Result<RepositoryCloneResult, PodbotError>>,
}

impl RepositoryCloningE2eState {
    /// Finish a completed scenario only when its container bundle is solely owned.
    ///
    /// For example, `state.finish()` removes its bundle and checks engine absence;
    /// a missing bundle or a retained step reference returns an ownership error.
    /// Scenario test bodies call this after their registered steps have finished.
    ///
    /// # Errors
    ///
    /// Returns an error if the bundle is missing or still shared, container
    /// removal or same-ID absence verification fails, or receipt output fails.
    pub fn finish(&self) -> StepResult<()> {
        let shared = self
            .bundle
            .take()
            .ok_or_else(|| String::from("sandbox bundle missing at scenario cleanup"))?;
        let bundle = Arc::try_unwrap(shared)
            .map_err(|_| String::from("sandbox bundle still shared at scenario cleanup"))?;
        bundle.finish()
    }
}

/// Fixture providing fresh scenario state.
#[fixture]
pub fn repository_cloning_e2e_state() -> RepositoryCloningE2eState {
    RepositoryCloningE2eState::default()
}

#[cfg(test)]
mod tests {
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
    enum ReceiptFailure {
        Write,
        Flush,
    }

    /// Inject an output failure without changing process stdout or engine state.
    struct FailingReceiptWriter {
        failure: ReceiptFailure,
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
}
