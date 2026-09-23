//! Container-creation test doubles and the queries that read what they captured.
//!
//! The mock and its capture helpers live here so the test module that uses
//! them stays inside the 400-line module limit, following the sibling-module
//! convention already used by `minimal_mode` and `privileged_mode`.

use std::sync::{Arc, Mutex, PoisonError};

use bollard::models::ContainerCreateResponse;
use mockall::mock;

use super::super::*;

mock! {
    #[derive(Debug)]
    pub(super) Creator {}

    impl ContainerCreator for Creator {
        fn create_container<'a>(
            &'a self,
            options: Option<CreateContainerOptions>,
            config: ContainerCreateBody,
        ) -> CreateContainerFuture<'a>;
    }
}

#[derive(Debug, Default)]
pub(super) struct CapturedCreateCall {
    call_count: usize,
    options: Option<CreateContainerOptions>,
    body: Option<ContainerCreateBody>,
}

/// Builds the error a mock returns once its single queued response is gone.
///
/// The doubles here answer exactly one call. Reporting a second call as an
/// engine error rather than ending the process keeps the helper free of a
/// verdict: the test still sees the extra call through `call_count`, and the
/// failure it reads is the one it asserted on.
pub(super) fn exhausted_double_error(operation: &str) -> bollard::errors::Error {
    bollard::errors::Error::IOError {
        err: std::io::Error::other(format!(
            "the {operation} double was already called; it answers one call"
        )),
    }
}

pub(super) fn creator_with_result(
    result: Result<ContainerCreateResponse, bollard::errors::Error>,
) -> (MockCreator, Arc<Mutex<CapturedCreateCall>>) {
    let mut creator = MockCreator::new();
    let captured = Arc::new(Mutex::new(CapturedCreateCall::default()));
    let captured_for_closure = Arc::clone(&captured);
    let response_state = Arc::new(Mutex::new(Some(result)));
    let response_state_for_closure = Arc::clone(&response_state);

    creator
        .expect_create_container()
        .returning(move |options, config| {
            {
                let mut captured_locked = captured_for_closure
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                captured_locked.call_count += 1;
                captured_locked.options = options;
                captured_locked.body = Some(config);
            }

            let response = response_state_for_closure
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .unwrap_or_else(|| Err(exhausted_double_error("container creation")));

            Box::pin(async move { response })
        });

    (creator, captured)
}

pub(super) fn success_creator(container_id: &str) -> (MockCreator, Arc<Mutex<CapturedCreateCall>>) {
    creator_with_result(Ok(ContainerCreateResponse {
        id: String::from(container_id),
        warnings: vec![],
    }))
}

pub(super) fn failing_creator(
    error: bollard::errors::Error,
) -> (MockCreator, Arc<Mutex<CapturedCreateCall>>) {
    creator_with_result(Err(error))
}

pub(super) fn take_options(
    captured: &Arc<Mutex<CapturedCreateCall>>,
) -> Option<CreateContainerOptions> {
    captured
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .options
        .clone()
}

pub(super) fn take_body(captured: &Arc<Mutex<CapturedCreateCall>>) -> Option<ContainerCreateBody> {
    captured
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .body
        .clone()
}

pub(super) fn call_count(captured: &Arc<Mutex<CapturedCreateCall>>) -> usize {
    captured
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .call_count
}
