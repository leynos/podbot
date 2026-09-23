//! Container-creation test doubles and the queries that read what they captured.
//!
//! The mock and its capture helpers live here so the test module that uses
//! them stays inside the 400-line module limit, following the sibling-module
//! convention already used by `minimal_mode` and `privileged_mode`.

use std::sync::{Arc, Mutex, MutexGuard};

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

/// Builds the error a double reports when one of its locks was poisoned.
///
/// A lock here is poisoned only by a panic while it was held, and that panic
/// has already failed the test. Reporting the poisoned lock as an error rather
/// than reading through it keeps the test from asserting on state a panicking
/// holder may have left half-written.
fn poisoned_lock_error(lock: &str) -> std::io::Error {
    std::io::Error::other(format!(
        "the {lock} lock was poisoned by a panicking holder"
    ))
}

/// Locks the captured call, reporting a poisoned lock as an I/O error.
fn lock_captured(
    captured: &Arc<Mutex<CapturedCreateCall>>,
) -> std::io::Result<MutexGuard<'_, CapturedCreateCall>> {
    captured
        .lock()
        .map_err(|_| poisoned_lock_error("captured create call"))
}

/// Records one call to the double: its options, its body, and the count.
fn record_call(
    captured: &Arc<Mutex<CapturedCreateCall>>,
    options: Option<CreateContainerOptions>,
    config: ContainerCreateBody,
) -> Result<(), bollard::errors::Error> {
    let mut call =
        lock_captured(captured).map_err(|err| bollard::errors::Error::IOError { err })?;
    call.call_count += 1;
    call.options = options;
    call.body = Some(config);
    Ok(())
}

/// Takes the double's single queued response, or reports it already used.
fn next_response(
    response_state: &Mutex<Option<Result<ContainerCreateResponse, bollard::errors::Error>>>,
) -> Result<ContainerCreateResponse, bollard::errors::Error> {
    response_state
        .lock()
        .map_err(|_| bollard::errors::Error::IOError {
            err: poisoned_lock_error("queued response"),
        })?
        .take()
        .unwrap_or_else(|| Err(exhausted_double_error("container creation")))
}

/// Builds a creator double answering one call with `result`, and the capture
/// of what that call was given.
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
            let response = record_call(&captured_for_closure, options, config)
                .and_then(|()| next_response(&response_state_for_closure));
            Box::pin(async move { response })
        });

    (creator, captured)
}

/// Builds a creator double that creates a container with `container_id`.
pub(super) fn success_creator(container_id: &str) -> (MockCreator, Arc<Mutex<CapturedCreateCall>>) {
    creator_with_result(Ok(ContainerCreateResponse {
        id: String::from(container_id),
        warnings: vec![],
    }))
}

/// Builds a creator double that fails with `error`.
pub(super) fn failing_creator(
    error: bollard::errors::Error,
) -> (MockCreator, Arc<Mutex<CapturedCreateCall>>) {
    creator_with_result(Err(error))
}

/// Returns the options the double was called with, if it was called.
pub(super) fn take_options(
    captured: &Arc<Mutex<CapturedCreateCall>>,
) -> std::io::Result<Option<CreateContainerOptions>> {
    Ok(lock_captured(captured)?.options.clone())
}

/// Returns the body the double was called with, if it was called.
pub(super) fn take_body(
    captured: &Arc<Mutex<CapturedCreateCall>>,
) -> std::io::Result<Option<ContainerCreateBody>> {
    Ok(lock_captured(captured)?.body.clone())
}

/// Returns how many times the double was called.
pub(super) fn call_count(captured: &Arc<Mutex<CapturedCreateCall>>) -> std::io::Result<usize> {
    Ok(lock_captured(captured)?.call_count)
}
