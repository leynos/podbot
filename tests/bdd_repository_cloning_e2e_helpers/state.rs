//! Shared state and fixture for the end-to-end repository-cloning scenarios.

use std::future::Future;
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
        let removal_future = async {
            container.rm().await.map_err(|error| {
                format!(
                    "failed to remove owned container {}: {error}",
                    self.container_id
                )
            })
        };
        finish_and_record(
            &self.container_id,
            || {
                self.runtime.block_on(remove_and_verify(
                    &self.container_id,
                    removal_future,
                    |owned_id| self.docker.inspect_container(owned_id, None),
                ))
            },
            emit_container_receipt,
        )
    }

    /// Emit ownership evidence only after this runtime-safe bundle exists.
    ///
    /// For example, `bundle.record_created()` writes one created record or
    /// returns an output error; the caller's bundle then retains RAII cleanup.
    pub(super) fn record_created(&self) -> StepResult<()> {
        self.record_created_to(&mut io::stdout().lock())
    }

    /// Record creation through the same fallible writer path used by stdout.
    ///
    /// Only the cloning harness and its output-failure regressions use this seam.
    fn record_created_to(&self, writer: &mut impl Write) -> StepResult<()> {
        record_container_receipt(writer, ContainerReceiptStage::Created, &self.container_id)
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
    record_container_receipt(&mut io::stdout().lock(), stage, container_id)
}

/// Preserve receipt failures at the lifecycle caller boundary.
///
/// Both creation and verified removal use this mapping; fake test IDs only reach
/// injected writers, never the default stdout adapter.
fn record_container_receipt(
    writer: &mut impl Write,
    stage: ContainerReceiptStage,
    container_id: &str,
) -> StepResult<()> {
    write_container_receipt(writer, stage, container_id, std::process::id())
        .map_err(|error| format!("failed to record owned container {container_id}: {error}"))
}

/// Remove first, then inspect the captured owned ID using the same runtime task.
///
/// This private harness seam reuses the real absence verdict. Its regressions
/// supply controlled operations without changing the production engine client.
async fn remove_and_verify<'id, Removal, Inspect, Inspection>(
    container_id: &'id str,
    removal: Removal,
    inspect: Inspect,
) -> StepResult<()>
where
    Removal: Future<Output = StepResult<()>>,
    Inspect: FnOnce(&'id str) -> Inspection,
    Inspection: Future<Output = Result<ContainerInspectResponse, BollardError>>,
{
    removal.await?;
    let inspection = inspect(container_id).await;
    require_container_absence(container_id, &inspection)
}

/// Record removal only after the actual cleanup boundary has completed.
///
/// Bundle cleanup and its composed regressions share this ordering. The caller
/// retains one `block_on`; its stdout adapter is acquired only after it returns.
fn finish_and_record(
    container_id: &str,
    cleanup: impl FnOnce() -> StepResult<()>,
    record: impl FnOnce(ContainerReceiptStage, &str) -> StepResult<()>,
) -> StepResult<()> {
    cleanup()?;
    record(ContainerReceiptStage::Removed, container_id)
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
#[path = "state_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;
