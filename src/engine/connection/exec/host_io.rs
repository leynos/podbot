//! Shared host-stdio boundary helpers for exec session forwarding.
//!
//! Unit tests always receive `tokio::io::empty()` so they never consume the
//! runner's stdin. In non-test builds, integration tests can force the same
//! empty reader by setting `PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS=1`;
//! otherwise Podbot forwards the real host stdin.

use std::pin::Pin;

use tokio::io::AsyncRead;

/// Fixed integration-test override read only through the injected environment.
const DISABLE_STDIN_FORWARDING_ENV: &str = "PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS";

/// Returns `false` in test builds; stdin forwarding is always treated as
/// enabled at this layer so that `default_host_stdin()` returns the
/// appropriate empty reader via its own `#[cfg(test)]` variant.
#[cfg(test)]
pub(super) const fn stdin_forwarding_disabled_for_tests() -> bool {
    false
}

/// Returns `true` when the `PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS`
/// environment variable is set to `"1"`, allowing integration tests to
/// suppress real stdin forwarding without a rebuild.
#[cfg(not(test))]
pub(super) fn stdin_forwarding_disabled_for_tests() -> bool {
    stdin_forwarding_disabled_with_env(&mockable::DefaultEnv::new())
}

/// Query the existing environment reader without broadening the override syntax.
///
/// For example, a reader returning `Ok(String::from("1"))` disables forwarding;
/// missing or non-Unicode values leave it enabled. Only the host-stdio boundary
/// and its deterministic regressions use this uncached query.
fn stdin_forwarding_disabled_with_env<E: mockable::Env>(env: &E) -> bool {
    env.raw(DISABLE_STDIN_FORWARDING_ENV)
        .is_ok_and(|value| value == "1")
}

/// Returns a deterministic empty stdin reader for unit tests.
///
/// Ensures unit tests never block on or consume the test runner's real stdin.
#[cfg(test)]
pub(super) fn default_host_stdin() -> Pin<Box<dyn AsyncRead + Send>> {
    Box::pin(tokio::io::empty())
}

/// Returns host stdin unless stdin forwarding is explicitly disabled.
///
/// When `PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS=1` is set, returns
/// `tokio::io::empty()` instead of the real `tokio::io::stdin()`.
#[cfg(not(test))]
pub(super) fn default_host_stdin() -> Pin<Box<dyn AsyncRead + Send>> {
    if stdin_forwarding_disabled_for_tests() {
        return Box::pin(tokio::io::empty());
    }
    Box::pin(tokio::io::stdin())
}

#[cfg(test)]
mod tests {
    //! Unit tests for host IO stdin-forwarding overrides.

    use std::env::VarError;
    use std::ffi::OsString;

    use mockable::MockEnv;
    use rstest::rstest;
    use tokio::io::AsyncReadExt;

    use super::*;

    #[tokio::test]
    async fn default_host_stdin_returns_empty_reader_in_tests() {
        let mut stdin = default_host_stdin();
        let mut bytes = Vec::new();
        let bytes_read = stdin
            .read_to_end(&mut bytes)
            .await
            .expect("test stdin reader should be readable");

        assert_eq!(bytes_read, 0, "test stdin should be empty");
        assert!(bytes.is_empty(), "test stdin should not yield any bytes");
    }

    #[rstest]
    #[case::exact_one(Ok(String::from("1")), true)]
    #[case::absent(Err(VarError::NotPresent), false)]
    #[case::empty(Ok(String::new()), false)]
    #[case::zero(Ok(String::from("0")), false)]
    #[case::boolean_word(Ok(String::from("true")), false)]
    #[case::leading_zero(Ok(String::from("01")), false)]
    #[case::leading_space(Ok(String::from(" 1")), false)]
    #[case::trailing_space(Ok(String::from("1 ")), false)]
    #[case::non_ascii_one(Ok(String::from("１")), false)]
    #[case::native_error(
        Err(VarError::NotUnicode(OsString::from("opaque native payload"))),
        false
    )]
    fn only_exact_one_disables_forwarding(
        #[case] value: Result<String, VarError>,
        #[case] expected: bool,
    ) {
        let mut env = MockEnv::new();
        env.expect_raw()
            .withf(|key| key == "PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS")
            .times(1)
            .return_once(move |_| value);

        let disabled = stdin_forwarding_disabled_with_env(&env);

        assert_eq!(
            disabled, expected,
            "only the exact Unicode value 1 disables stdin"
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn non_unicode_native_values_leave_forwarding_enabled() {
        #[cfg(unix)]
        use std::os::unix::ffi::OsStringExt;
        #[cfg(windows)]
        use std::os::windows::ffi::OsStringExt;

        #[cfg(unix)]
        let native_value = OsString::from_vec(vec![0xff]);
        #[cfg(windows)]
        let native_value = OsString::from_wide(&[0xd800]);

        assert!(
            native_value.to_str().is_none(),
            "fixture must retain non-Unicode native data"
        );
        let mut env = MockEnv::new();
        env.expect_raw()
            .withf(|key| key == "PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS")
            .times(1)
            .return_once(move |_| Err(VarError::NotUnicode(native_value)));

        let disabled = stdin_forwarding_disabled_with_env(&env);

        assert!(
            !disabled,
            "a native decoding error must preserve the default forwarding path"
        );
    }
}
