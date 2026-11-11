//! Container listing and stop orchestration.
//!
//! `podbot ps` and `podbot stop` both operate only on containers carrying the
//! podbot managed label. Listing maps engine records into a stable
//! [`ContainerSummary`] shape so the CLI adapter can format output without
//! depending on `Bollard` types. Stopping inspects the container first, refuses
//! containers podbot does not own, and treats an already-stopped container as
//! success.

use bollard::models::{ContainerInspectResponse, ContainerSummary as EngineContainerSummary};

#[cfg(feature = "experimental")]
use crate::config::AppConfig;
use crate::engine::{
    BRANCH_LABEL_KEY, ContainerLifecycleClient, EngineConnector, REPOSITORY_LABEL_KEY, is_managed,
    label_value, managed_label_filter,
};
use crate::error::{ConfigError, ContainerError, Result as PodbotResult};

use super::CommandOutcome;

/// Seconds the engine waits for a container to exit before killing it.
const STOP_GRACE_SECONDS: i32 = 10;

/// A podbot-managed container as presented to CLI and library callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerSummary {
    /// Engine-assigned container identifier.
    pub id: String,
    /// Primary container name without the engine's leading slash.
    pub name: String,
    /// Image reference recorded at creation time.
    pub image: String,
    /// Engine state such as `running` or `exited`.
    pub state: String,
    /// Human-readable status line reported by the engine.
    pub status: String,
    /// Creation time as Unix seconds, when the engine reports it.
    pub created_unix_seconds: Option<i64>,
    /// Repository recorded in the ownership labels.
    pub repository: Option<String>,
    /// Branch recorded in the ownership labels.
    pub branch: Option<String>,
}

/// List running podbot-managed containers.
///
/// Results are ordered newest first so `podbot ps` shows the most recent
/// session at the top.
///
/// # Errors
///
/// Returns runtime creation, engine connection, or
/// `ContainerError::ListFailed` errors.
#[cfg(feature = "experimental")]
pub fn list_containers(config: &AppConfig) -> PodbotResult<Vec<ContainerSummary>> {
    let runtime = super::create_runtime()?;
    let docker = super::connect_engine(config)?;
    list_containers_with_client(&docker, runtime.handle())
}

/// List managed containers using an injected client and runtime handle.
///
/// # Errors
///
/// Returns `ContainerError::ListFailed` when the engine rejects the request.
pub fn list_containers_with_client<C: ContainerLifecycleClient + Sync>(
    client: &C,
    runtime: &tokio::runtime::Handle,
) -> PodbotResult<Vec<ContainerSummary>> {
    let filter = managed_label_filter();
    let summaries = EngineConnector::list_managed_containers(runtime, client, &filter)?;
    Ok(sorted_summaries(summaries))
}

/// Stop a running podbot-managed container.
///
/// Stopping an already-stopped container succeeds without contacting the stop
/// endpoint. Containers without the managed label are refused so a mistyped
/// identifier cannot stop an unrelated workload.
///
/// # Errors
///
/// Returns `ConfigError::MissingRequired` for a blank identifier,
/// `ContainerError::NotFound` for an unknown container,
/// `ContainerError::NotManaged` for a container podbot does not own, and
/// `ContainerError::StopFailed` for other engine failures.
#[cfg(feature = "experimental")]
pub fn stop_container(config: &AppConfig, container: &str) -> PodbotResult<CommandOutcome> {
    let validated = validate_container_identifier(container)?;
    let runtime = super::create_runtime()?;
    let docker = super::connect_engine(config)?;
    stop_container_with_client(&docker, runtime.handle(), validated)
}

/// Stop a container using an injected client and runtime handle.
///
/// # Errors
///
/// Returns the same errors as [`stop_container`].
pub fn stop_container_with_client<C: ContainerLifecycleClient + Sync>(
    client: &C,
    runtime: &tokio::runtime::Handle,
    container: &str,
) -> PodbotResult<CommandOutcome> {
    let inspected = EngineConnector::inspect_container(runtime, client, container)?;
    ensure_managed(container, &inspected)?;

    if needs_stop(&inspected) {
        EngineConnector::stop_container(runtime, client, container, STOP_GRACE_SECONDS)?;
    }

    Ok(CommandOutcome::Success)
}

/// Reject blank identifiers before any engine work starts.
fn validate_container_identifier(container: &str) -> PodbotResult<&str> {
    if container.trim().is_empty() {
        return Err(ConfigError::MissingRequired {
            field: String::from("container"),
        }
        .into());
    }
    Ok(container)
}

/// Refuse containers that do not carry the podbot managed label.
fn ensure_managed(container: &str, inspected: &ContainerInspectResponse) -> PodbotResult<()> {
    let labels = inspected
        .config
        .as_ref()
        .and_then(|config| config.labels.as_ref());

    if is_managed(labels) {
        Ok(())
    } else {
        Err(ContainerError::NotManaged {
            container: String::from(container),
        }
        .into())
    }
}

/// Treat an unknown running state as needing a stop request; the engine
/// reports `304 Not Modified` for containers that are already stopped.
fn needs_stop(inspected: &ContainerInspectResponse) -> bool {
    inspected.state.as_ref().and_then(|state| state.running) != Some(false)
}

/// Map engine records and order them newest first.
fn sorted_summaries(summaries: Vec<EngineContainerSummary>) -> Vec<ContainerSummary> {
    let mut mapped: Vec<_> = summaries.into_iter().map(map_summary).collect();
    mapped.sort_by(|left, right| {
        right
            .created_unix_seconds
            .cmp(&left.created_unix_seconds)
            .then_with(|| left.id.cmp(&right.id))
    });
    mapped
}

/// Convert one engine record into the stable summary shape.
fn map_summary(summary: EngineContainerSummary) -> ContainerSummary {
    let labels = summary.labels.as_ref();

    ContainerSummary {
        id: summary.id.unwrap_or_default(),
        name: primary_name(summary.names),
        image: summary.image.unwrap_or_default(),
        state: summary
            .state
            .map(|state| state.to_string())
            .unwrap_or_default(),
        status: summary.status.unwrap_or_default(),
        created_unix_seconds: summary.created,
        repository: label_value(labels, REPOSITORY_LABEL_KEY).map(String::from),
        branch: label_value(labels, BRANCH_LABEL_KEY).map(String::from),
    }
}

/// Return the first engine name without its historical leading slash.
fn primary_name(names: Option<Vec<String>>) -> String {
    names
        .and_then(|entries| entries.into_iter().next())
        .map(|name| name.trim_start_matches('/').to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
