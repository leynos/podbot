//! Unit tests for container lifecycle operations.

use bollard::models::{ContainerInspectResponse, ContainerSummary};
use mockall::mock;
use rstest::rstest;

use super::*;

mock! {
    LifecycleClient {}
    impl ContainerLifecycleClient for LifecycleClient {
        fn start_container(&self, container_id: &str) -> StartContainerFuture<'_>;
        fn stop_container(
            &self,
            container_id: &str,
            grace_seconds: i32,
        ) -> StopContainerFuture<'_>;
        fn remove_container(&self, container_id: &str, force: bool) -> RemoveContainerFuture<'_>;
        fn inspect_container(&self, container_id: &str) -> InspectContainerFuture<'_>;
        fn list_containers(&self, label_filter: &str) -> ListContainersFuture<'_>;
    }
}

fn server_error(status_code: u16) -> bollard::errors::Error {
    bollard::errors::Error::DockerResponseServerError {
        status_code,
        message: String::from("engine rejected request"),
    }
}

fn summary(id: &str) -> ContainerSummary {
    ContainerSummary {
        id: Some(String::from(id)),
        ..ContainerSummary::default()
    }
}

#[tokio::test]
async fn start_success_returns_unit() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_start_container()
        .withf(|container_id| container_id == "ctr")
        .times(1)
        .returning(|_| Box::pin(async { Ok(()) }));

    let result = EngineConnector::start_container_async(&client, "ctr").await;

    assert!(result.is_ok(), "expected start success, got {result:?}");
}

#[tokio::test]
async fn start_failure_maps_to_start_failed() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_start_container()
        .returning(|_| Box::pin(async { Err(bollard::errors::Error::RequestTimeoutError) }));

    let result = EngineConnector::start_container_async(&client, "ctr").await;

    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::StartFailed {
                ref container_id,
                ..
            })) if container_id == "ctr"
        ),
        "expected StartFailed, got {result:?}"
    );
}

#[tokio::test]
async fn stop_success_reports_stopped() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_stop_container()
        .withf(|container_id, grace| container_id == "ctr" && *grace == 10)
        .times(1)
        .returning(|_, _| Box::pin(async { Ok(()) }));

    let outcome = EngineConnector::stop_container_async(&client, "ctr", 10)
        .await
        .expect("stop should succeed");

    assert_eq!(outcome, StopOutcome::Stopped);
}

#[tokio::test]
async fn stop_not_modified_reports_already_stopped() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_stop_container()
        .returning(|_, _| Box::pin(async { Err(server_error(304)) }));

    let outcome = EngineConnector::stop_container_async(&client, "ctr", 10)
        .await
        .expect("304 should be a successful already-stopped outcome");

    assert_eq!(outcome, StopOutcome::AlreadyStopped);
}

#[tokio::test]
async fn stop_not_found_maps_to_not_found() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_stop_container()
        .returning(|_, _| Box::pin(async { Err(server_error(404)) }));

    let result = EngineConnector::stop_container_async(&client, "ctr", 10).await;

    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::NotFound { ref container }))
                if container == "ctr"
        ),
        "expected NotFound, got {result:?}"
    );
}

#[tokio::test]
async fn stop_failure_maps_to_stop_failed() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_stop_container()
        .returning(|_, _| Box::pin(async { Err(server_error(500)) }));

    let result = EngineConnector::stop_container_async(&client, "ctr", 10).await;

    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::StopFailed { .. }))
        ),
        "expected StopFailed, got {result:?}"
    );
}

#[tokio::test]
async fn remove_success_returns_unit() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_remove_container()
        .withf(|container_id, force| container_id == "ctr" && *force)
        .times(1)
        .returning(|_, _| Box::pin(async { Ok(()) }));

    let result = EngineConnector::remove_container_async(&client, "ctr").await;

    assert!(result.is_ok(), "expected remove success, got {result:?}");
}

#[rstest]
#[case::not_found(404, true)]
#[case::other_failure(500, false)]
#[tokio::test]
async fn remove_error_maps_by_status(#[case] status: u16, #[case] is_not_found: bool) {
    let mut client = MockLifecycleClient::new();
    client
        .expect_remove_container()
        .returning(move |_, _| Box::pin(async move { Err(server_error(status)) }));

    let result = EngineConnector::remove_container_async(&client, "ctr").await;

    if is_not_found {
        assert!(
            matches!(
                result,
                Err(PodbotError::Container(ContainerError::NotFound { .. }))
            ),
            "expected NotFound, got {result:?}"
        );
    } else {
        assert!(
            matches!(
                result,
                Err(PodbotError::Container(ContainerError::RemoveFailed { .. }))
            ),
            "expected RemoveFailed, got {result:?}"
        );
    }
}

#[tokio::test]
async fn inspect_success_returns_response() {
    let mut client = MockLifecycleClient::new();
    client.expect_inspect_container().returning(|_| {
        Box::pin(async {
            Ok(ContainerInspectResponse {
                id: Some(String::from("ctr")),
                ..ContainerInspectResponse::default()
            })
        })
    });

    let response = EngineConnector::inspect_container_async(&client, "ctr")
        .await
        .expect("inspect should succeed");

    assert_eq!(response.id.as_deref(), Some("ctr"));
}

#[rstest]
#[case::not_found(404, true)]
#[case::other_failure(500, false)]
#[tokio::test]
async fn inspect_error_maps_by_status(#[case] status: u16, #[case] is_not_found: bool) {
    let mut client = MockLifecycleClient::new();
    client
        .expect_inspect_container()
        .returning(move |_| Box::pin(async move { Err(server_error(status)) }));

    let result = EngineConnector::inspect_container_async(&client, "ctr").await;

    if is_not_found {
        assert!(
            matches!(
                result,
                Err(PodbotError::Container(ContainerError::NotFound { .. }))
            ),
            "expected NotFound, got {result:?}"
        );
    } else {
        assert!(
            matches!(
                result,
                Err(PodbotError::Container(ContainerError::InspectFailed { .. }))
            ),
            "expected InspectFailed, got {result:?}"
        );
    }
}

#[tokio::test]
async fn list_success_returns_containers() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_list_containers()
        .withf(|filter| filter == "io.podbot.managed=true")
        .times(1)
        .returning(|_| Box::pin(async { Ok(vec![summary("ctr-1"), summary("ctr-2")]) }));

    let containers =
        EngineConnector::list_managed_containers_async(&client, "io.podbot.managed=true")
            .await
            .expect("list should succeed");

    assert_eq!(containers.len(), 2);
}

#[tokio::test]
async fn list_failure_maps_to_list_failed() {
    let mut client = MockLifecycleClient::new();
    client
        .expect_list_containers()
        .returning(|_| Box::pin(async { Err(bollard::errors::Error::RequestTimeoutError) }));

    let result =
        EngineConnector::list_managed_containers_async(&client, "io.podbot.managed=true").await;

    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::ListFailed { .. }))
        ),
        "expected ListFailed, got {result:?}"
    );
}
