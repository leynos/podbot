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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_failed_produces_container_exec_failed_error() {
        let err = exec_failed("my-container", "something went wrong");
        assert!(
            matches!(
                err,
                crate::error::PodbotError::Container(
                    crate::error::ContainerError::ExecFailed {
                        ref container_id,
                        ref message,
                    }
                ) if container_id == "my-container" && message == "something went wrong"
            ),
            "unexpected error: {err:?}",
        );
    }

    #[test]
    fn exec_failed_accepts_string_owned_message() {
        let msg = String::from("owned message");
        let err = exec_failed("ctr", msg);
        assert!(
            matches!(
                err,
                crate::error::PodbotError::Container(
                    crate::error::ContainerError::ExecFailed { ref message, .. }
                ) if message == "owned message"
            ),
            "unexpected error: {err:?}",
        );
    }

    #[test]
    fn block_on_runtime_returns_ok_result_outside_tokio() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should be created");
        let handle = rt.handle().clone();

        let result: Result<u32, crate::error::PodbotError> =
            block_on_runtime(&handle, async { Ok(42_u32) });

        assert_eq!(result.expect("future should resolve to Ok(42)"), 42);
    }

    #[test]
    fn block_on_runtime_propagates_err_result_outside_tokio() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should be created");
        let handle = rt.handle().clone();

        let result: Result<(), crate::error::PodbotError> =
            block_on_runtime(&handle, async { Err(exec_failed("c", "injected error")) });

        let err = result.expect_err("future should resolve to Err");
        assert!(
            matches!(
                err,
                crate::error::PodbotError::Container(
                    crate::error::ContainerError::ExecFailed { ref message, .. }
                ) if message == "injected error"
            ),
            "unexpected error: {err:?}",
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn block_on_runtime_resolves_future_inside_tokio_context() {
        let handle = tokio::runtime::Handle::current();

        let result: Result<u32, crate::error::PodbotError> =
            block_on_runtime(&handle, async { Ok(99_u32) });

        assert_eq!(
            result.expect("future should resolve to Ok(99) inside Tokio"),
            99,
        );
    }
}
