//! Unit tests for container listing and stop orchestration.

use std::collections::HashMap;

use bollard::models::{
    ContainerConfig, ContainerInspectResponse, ContainerState, ContainerSummary as EngineSummary,
    ContainerSummaryStateEnum,
};
use mockall::mock;

use crate::engine::{MANAGED_LABEL_KEY, MANAGED_LABEL_VALUE};
use crate::error::{ConfigError, ContainerError, PodbotError};

use super::*;

mock! {
    LifecycleClient {}
    impl ContainerLifecycleClient for LifecycleClient {
        fn start_container(&self, container_id: &str) -> crate::engine::StartContainerFuture<'_>;
        fn stop_container(
            &self,
            container_id: &str,
            grace_seconds: i32,
        ) -> crate::engine::StopContainerFuture<'_>;
        fn remove_container(
            &self,
            container_id: &str,
            force: bool,
        ) -> crate::engine::RemoveContainerFuture<'_>;
        fn inspect_container(
            &self,
            container_id: &str,
        ) -> crate::engine::InspectContainerFuture<'_>;
        fn list_containers(
            &self,
            label_filter: &str,
        ) -> crate::engine::ListContainersFuture<'_>;
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should build")
}

fn engine_summary(id: &str, name: &str, created: i64) -> EngineSummary {
    EngineSummary {
        id: Some(String::from(id)),
        names: Some(vec![format!("/{name}")]),
        image: Some(String::from("ghcr.io/example/podbot:latest")),
        state: Some(ContainerSummaryStateEnum::RUNNING),
        status: Some(String::from("Up 2 minutes")),
        created: Some(created),
        labels: Some(HashMap::from([
            (
                String::from(MANAGED_LABEL_KEY),
                String::from(MANAGED_LABEL_VALUE),
            ),
            (
                String::from(REPOSITORY_LABEL_KEY),
                String::from("owner/name"),
            ),
            (String::from(BRANCH_LABEL_KEY), String::from("main")),
        ])),
        ..EngineSummary::default()
    }
}

fn inspect_response(running: bool, managed: bool) -> ContainerInspectResponse {
    let mut labels = HashMap::new();
    if managed {
        labels.insert(
            String::from(MANAGED_LABEL_KEY),
            String::from(MANAGED_LABEL_VALUE),
        );
    }

    ContainerInspectResponse {
        id: Some(String::from("ctr")),
        state: Some(ContainerState {
            running: Some(running),
            ..ContainerState::default()
        }),
        config: Some(ContainerConfig {
            labels: Some(labels),
            ..ContainerConfig::default()
        }),
        ..ContainerInspectResponse::default()
    }
}

#[test]
fn list_maps_engine_records_newest_first() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_list_containers()
        .withf(|filter| filter == "io.podbot.managed=true")
        .times(1)
        .returning(|_| {
            Box::pin(async {
                Ok(vec![
                    engine_summary("old", "podbot-old", 10),
                    engine_summary("new", "podbot-new", 20),
                ])
            })
        });

    let runtime = runtime();
    let containers =
        list_containers_with_client(&client, runtime.handle()).expect("listing should succeed");

    assert_eq!(containers.len(), 2);
    let newest = containers
        .first()
        .expect("newest container should be present");
    let oldest = containers
        .get(1)
        .expect("oldest container should be present");
    assert_eq!(newest.id, "new");
    assert_eq!(newest.name, "podbot-new");
    assert_eq!(newest.state, "running");
    assert_eq!(newest.repository.as_deref(), Some("owner/name"));
    assert_eq!(newest.branch.as_deref(), Some("main"));
    assert_eq!(oldest.id, "old");
}

#[test]
fn list_returns_empty_when_no_managed_containers_exist() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_list_containers()
        .returning(|_| Box::pin(async { Ok(Vec::new()) }));

    let runtime = runtime();
    let containers = list_containers_with_client(&client, runtime.handle())
        .expect("empty listing should succeed");

    assert_eq!(containers, Vec::new());
}

#[test]
fn list_propagates_engine_failure() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_list_containers()
        .returning(|_| Box::pin(async { Err(bollard::errors::Error::RequestTimeoutError) }));

    let runtime = runtime();
    let result = list_containers_with_client(&client, runtime.handle());

    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::ListFailed { .. }))
        ),
        "expected ListFailed, got {result:?}"
    );
}

#[test]
fn stop_running_managed_container_requests_stop() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_inspect_container()
        .withf(|container| container == "ctr")
        .times(1)
        .returning(|_| Box::pin(async { Ok(inspect_response(true, true)) }));
    client
        .expect_stop_container()
        .withf(|container, grace| container == "ctr" && *grace == STOP_GRACE_SECONDS)
        .times(1)
        .returning(|_, _| Box::pin(async { Ok(()) }));

    let runtime = runtime();
    let outcome =
        stop_container_with_client(&client, runtime.handle(), "ctr").expect("stop should succeed");

    assert_eq!(outcome, CommandOutcome::Success);
}

#[test]
fn stop_already_stopped_container_is_noop_success() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_inspect_container()
        .returning(|_| Box::pin(async { Ok(inspect_response(false, true)) }));
    client.expect_stop_container().times(0);

    let runtime = runtime();
    let outcome = stop_container_with_client(&client, runtime.handle(), "ctr")
        .expect("already-stopped stop should succeed");

    assert_eq!(outcome, CommandOutcome::Success);
}

#[test]
fn stop_unmanaged_container_is_refused() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_inspect_container()
        .returning(|_| Box::pin(async { Ok(inspect_response(true, false)) }));
    client.expect_stop_container().times(0);

    let runtime = runtime();
    let result = stop_container_with_client(&client, runtime.handle(), "ctr");

    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::NotManaged { ref container }))
                if container == "ctr"
        ),
        "expected NotManaged, got {result:?}"
    );
}

#[test]
fn stop_unknown_container_reports_not_found() {
    let mut client = MockLifecycleClient::new();
    client.expect_inspect_container().returning(|_| {
        Box::pin(async {
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404,
                message: String::from("no such container"),
            })
        })
    });

    let runtime = runtime();
    let result = stop_container_with_client(&client, runtime.handle(), "ctr");

    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::NotFound { ref container }))
                if container == "ctr"
        ),
        "expected NotFound, got {result:?}"
    );
}

#[test]
fn blank_container_identifier_is_rejected_before_engine_work() {
    let result = validate_container_identifier("   ");

    assert!(
        matches!(
            result,
            Err(PodbotError::Config(ConfigError::MissingRequired { ref field }))
                if field == "container"
        ),
        "expected MissingRequired, got {result:?}"
    );
}
