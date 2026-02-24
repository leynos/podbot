//! Shared helpers for exec lifecycle test-state transitions.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use super::*;

pub(super) fn setup_inspect_exec_with_running_transition(
    client: &mut MockExecClient,
    exit_code: i64,
    running_checks: usize,
) {
    let call_index = Arc::new(AtomicUsize::new(0));
    let call_index_for_mock = Arc::clone(&call_index);
    client
        .expect_inspect_exec()
        .times(running_checks + 1)
        .returning(move |exec_id| {
            assert!(!exec_id.is_empty(), "exec id should be populated");
            let current_index = call_index_for_mock.fetch_add(1, Ordering::SeqCst);
            let response = if current_index < running_checks {
                bollard::models::ExecInspectResponse {
                    running: Some(true),
                    exit_code: None,
                    ..bollard::models::ExecInspectResponse::default()
                }
            } else {
                bollard::models::ExecInspectResponse {
                    running: Some(false),
                    exit_code: Some(exit_code),
                    ..bollard::models::ExecInspectResponse::default()
                }
            };
            Box::pin(async move { Ok(response) })
        });
}
