//! Blocking runtime helpers for synchronous exec wrappers.

use crate::error::{ContainerError, PodbotError};

pub(super) fn block_on_runtime<F, T>(
    runtime: &tokio::runtime::Handle,
    future: F,
) -> Result<T, PodbotError>
where
    F: std::future::Future<Output = Result<T, PodbotError>> + Send,
    T: Send,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|scope| {
            let blocking_task = scope.spawn(|| -> Result<T, PodbotError> {
                let blocking_runtime = create_blocking_exec_runtime()?;
                blocking_runtime.block_on(future)
            });

            match blocking_task.join() {
                Ok(output) => output,
                Err(panic) => std::panic::resume_unwind(panic),
            }
        })
    } else {
        runtime.block_on(future)
    }
}

fn create_blocking_exec_runtime() -> Result<tokio::runtime::Runtime, PodbotError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            PodbotError::from(ContainerError::RuntimeCreationFailed {
                message: error.to_string(),
            })
        })
}

pub(super) fn exec_failed(container_id: &str, message: impl Into<String>) -> PodbotError {
    PodbotError::from(ContainerError::ExecFailed {
        container_id: String::from(container_id),
        message: message.into(),
    })
}
