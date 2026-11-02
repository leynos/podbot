//! Scoped stdin selection for the interactive-exec and orchestration BDD suites.
//!
//! Only those two integration-test roots include this module; generic test
//! utilities must not pull its environment mutation into unrelated suites.

/// Shared scoped environment boundary for the complete synchronous operation.
#[path = "environment.rs"]
mod environment;

/// Run a complete synchronous exec operation without host stdin forwarding.
///
/// Any `block_on` call belongs inside `operation`; returning an
/// unpolled future would restore the environment before the operation runs.
/// Previous values are restored on success, error, or panic.
///
/// # Example
///
/// ```ignore
/// let outcome = with_stdin_forwarding_disabled(|| runtime.block_on(exec()));
/// ```
pub fn with_stdin_forwarding_disabled<F, R>(operation: F) -> R
where
    F: FnOnce() -> R,
{
    environment::with_test_environment("PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS", "1", operation)
}

#[cfg(test)]
mod tests {
    //! Directly specify the stdin-forwarding environment scope's restoration behaviour.

    use super::with_stdin_forwarding_disabled;
    use rstest::rstest;
    use std::env;

    #[rstest]
    #[case::absent_success(None, Ok("success sentinel"))]
    #[case::absent_error(None, Err("error sentinel"))]
    #[case::existing_success(Some("original value"), Ok("success sentinel"))]
    #[case::existing_error(Some("original value"), Err("error sentinel"))]
    fn restores_value_after_result(
        #[case] initial: Option<&str>,
        #[case] expected: Result<&str, &str>,
    ) {
        let run_case = || {
            let outcome = with_stdin_forwarding_disabled(|| {
                let value = env::var("PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS");
                assert_eq!(
                    value.as_deref(),
                    Ok("1"),
                    "the stdin-forwarding switch should be set during the operation"
                );
                expected
            });
            assert_eq!(
                outcome, expected,
                "the operation result should be preserved"
            );
        };

        match initial {
            None => temp_env::with_var_unset("PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS", || {
                run_case();
                assert!(
                    env::var_os("PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS").is_none(),
                    "the previously absent switch should be restored"
                );
            }),
            Some(original) => temp_env::with_var(
                "PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS",
                Some(original),
                || {
                    run_case();
                    let restored = env::var("PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS");
                    assert_eq!(
                        restored.as_deref(),
                        Ok(original),
                        "the prior switch value should be restored"
                    );
                },
            ),
        }
    }

    #[rstest]
    #[case::absent(None)]
    #[case::existing(Some("original value"))]
    fn restores_value_after_unwind(#[case] initial: Option<&str>) {
        let run_case = || {
            let outcome = std::panic::catch_unwind(|| {
                with_stdin_forwarding_disabled(|| {
                    let value = env::var("PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS");
                    assert_eq!(
                        value.as_deref(),
                        Ok("1"),
                        "the stdin-forwarding switch should be set during the operation"
                    );
                    panic!("unwind sentinel");
                });
            });

            let panic_payload = outcome.expect_err("the operation should unwind");
            let payload = panic_payload.downcast_ref::<&str>();
            assert_eq!(
                payload,
                Some(&"unwind sentinel"),
                "only the deliberate operation panic should be caught"
            );
        };

        match initial {
            None => temp_env::with_var_unset("PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS", || {
                run_case();
                assert!(
                    env::var_os("PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS").is_none(),
                    "the previously absent switch should be restored after unwinding"
                );
            }),
            Some(original) => temp_env::with_var(
                "PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS",
                Some(original),
                || {
                    run_case();
                    let restored = env::var("PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS");
                    assert_eq!(
                        restored.as_deref(),
                        Ok(original),
                        "the prior switch value should be restored after unwinding"
                    );
                },
            ),
        }
    }
}
