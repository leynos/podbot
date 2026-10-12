//! Shared `mockall` scaffolding for the exec-client behavioural suites.
//!
//! The `bdd_interactive_exec` and `bdd_orchestration` test crates both drive
//! the container exec seam through a `mockall` double of
//! `podbot::engine::ContainerExecClient`. Their mock declarations and their
//! create-exec, resize, and inspect expectation builders were duplicates apart
//! from the double's type name and the identifiers baked into the canned
//! responses, so they are generated here from a single macro.
//!
//! # Scope and re-use policy
//!
//! [`define_exec_client_mock!`] is intended solely for behavioural test crates
//! under `tests/` that exercise the exec seam. It expands into the invoking
//! module, so each crate keeps its own double and its own expectation
//! builders; nothing is shared at runtime. Start-exec expectations are
//! deliberately *not* generated: the two suites assert different daemon
//! options, so each crate owns that builder.
//!
//! Callers must have the following in scope at the expansion site:
//!
//! - `podbot::engine::{ContainerExecClient, CreateExecFuture, InspectExecFuture,
//!   ResizeExecFuture, StartExecFuture}`;
//! - the crate's `StepResult<T> = Result<T, String>` alias;
//! - the `ExecMode` enum passed as the `exec_mode` argument.

/// Generates an exec-client double along with its shared expectation builders.
///
/// The macro emits a `mockall` double named `Mock$mock_base`, plus
/// `configure_create_exec`, `configure_resize`, and `configure_inspect`
/// helpers that operate on it.
///
/// # Examples
///
/// ```ignore
/// crate::define_exec_client_mock! {
///     mock_base = ExecClient,
///     mock_type = MockExecClient,
///     exec_mode = ExecMode,
///     container_id = "bdd-sandbox",
///     exec_id = "bdd-exec-id",
/// }
///
/// let mut client = MockExecClient::new();
/// configure_create_exec(&mut client, false);
/// configure_inspect(&mut client, Some(0));
/// ```
#[macro_export]
macro_rules! define_exec_client_mock {
    (
        mock_base = $mock_base:ident,
        mock_type = $mock_type:ident,
        exec_mode = $exec_mode:ident,
        container_id = $container_id:literal,
        exec_id = $exec_id:literal $(,)?
    ) => {
        ::mockall::mock! {
            #[derive(Debug)]
            $mock_base {}

            impl ContainerExecClient for $mock_base {
                fn create_exec(
                    &self,
                    container_id: &str,
                    options: ::bollard::exec::CreateExecOptions<String>,
                ) -> CreateExecFuture<'_>;
                fn start_exec(
                    &self,
                    exec_id: &str,
                    options: Option<::bollard::exec::StartExecOptions>,
                ) -> StartExecFuture<'_>;
                fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
                fn resize_exec(
                    &self,
                    exec_id: &str,
                    options: ::bollard::exec::ResizeExecOptions,
                ) -> ResizeExecFuture<'_>;
            }
        }

        /// Expects exactly one `create_exec` call.
        ///
        /// When `should_fail` is set the daemon call times out, modelling an
        /// unreachable engine; otherwise the call must target the scenario's
        /// container and forward a command.
        fn configure_create_exec(client: &mut $mock_type, should_fail: bool) {
            if should_fail {
                client.expect_create_exec().times(1).returning(|_, _| {
                    Box::pin(async { Err(::bollard::errors::Error::RequestTimeoutError) })
                });
                return;
            }

            // `withf` pins the target container and forwarded command; a
            // mismatch surfaces as mockall's unmatched-expectation failure
            // rather than a panic inside this helper.
            client
                .expect_create_exec()
                .times(1)
                .withf(|container_id, options| {
                    container_id == $container_id && options.cmd.is_some()
                })
                .returning(|_, _| {
                    Box::pin(async {
                        Ok(::bollard::exec::CreateExecResults {
                            id: String::from($exec_id),
                        })
                    })
                });
        }

        /// Expects the resize behaviour appropriate to `mode`.
        ///
        /// Attached mode may still skip resizing when terminal dimensions are
        /// unavailable, so that expectation intentionally allows zero calls.
        fn configure_resize(client: &mut $mock_type, mode: $exec_mode) -> StepResult<()> {
            match mode {
                $exec_mode::Attached => {
                    client
                        .expect_resize_exec()
                        .times(0..)
                        .returning(|_, _| Box::pin(async { Ok(()) }));
                    Ok(())
                }
                $exec_mode::Detached | $exec_mode::Protocol => {
                    client.expect_resize_exec().never();
                    Ok(())
                }
                other => Err(format!(
                    "unexpected exec mode in resize expectation: {other:?}"
                )),
            }
        }

        /// Expects exactly one `inspect_exec` call reporting `exit_code`.
        ///
        /// Passing `None` models a daemon that omits the exit code entirely.
        fn configure_inspect(client: &mut $mock_type, exit_code: Option<i64>) {
            client.expect_inspect_exec().times(1).returning(move |_| {
                let inspect = ::bollard::models::ExecInspectResponse {
                    running: Some(false),
                    exit_code,
                    ..::bollard::models::ExecInspectResponse::default()
                };
                Box::pin(async move { Ok(inspect) })
            });
        }
    };
}
