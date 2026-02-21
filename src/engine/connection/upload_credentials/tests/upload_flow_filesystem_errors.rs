//! Filesystem-error credential upload scenarios.

use camino::{Utf8Path, Utf8PathBuf};
use rstest::rstest;

use super::*;
use crate::error::{FilesystemError, PodbotError};

#[derive(Clone, Copy)]
struct UploadFailureExpectation<'a> {
    container_id: &'a str,
    copy_claude: bool,
    copy_codex: bool,
    expected_message_substring: &'a str,
}

/// Helper to assert that credential upload fails with a host filesystem error.
fn assert_upload_fails_with_filesystem_error<F>(
    setup: F,
    expectation: UploadFailureExpectation<'_>,
) -> std::io::Result<()>
where
    F: FnOnce(&Utf8Path) -> std::io::Result<Utf8PathBuf>,
{
    let (_tmp, host_home) = host_home_dir()?;
    let host_home_path = setup(&host_home)?;
    let expected_path = host_home_path.as_std_path().to_path_buf();

    let request = CredentialUploadRequest::new(
        expectation.container_id,
        host_home_path,
        expectation.copy_claude,
        expectation.copy_codex,
    );
    let (uploader, captured) = successful_uploader();

    let result = runtime()?.block_on(EngineConnector::upload_credentials_async(
        &uploader, &request,
    ));
    let error = match result {
        Ok(success) => {
            return Err(io_error(format!(
                "expected upload failure, got success: {success:?}"
            )));
        }
        Err(error) => error,
    };

    ensure(
        matches!(
            error,
            PodbotError::Filesystem(FilesystemError::IoError {
                ref path,
                ref message,
            }) if path == &expected_path && message.contains(expectation.expected_message_substring)
        ),
        format!(
            concat!(
                "expected filesystem error with path={:?} ",
                "and message containing '{}', got: {:?}"
            ),
            expected_path, expectation.expected_message_substring, error
        ),
    )?;
    ensure(
        captured_call(&captured)?.call_count == 0,
        format!(
            "expected no daemon upload call for container '{}'",
            expectation.container_id
        ),
    )?;

    Ok(())
}

fn ensure(condition: bool, failure_message: impl Into<String>) -> std::io::Result<()> {
    if condition {
        return Ok(());
    }

    Err(io_error(failure_message))
}

macro_rules! filesystem_error_test {
    (
        $test_name:ident,
        $setup:expr,
        container_id = $container_id:literal,
        copy_claude = $copy_claude:expr,
        copy_codex = $copy_codex:expr,
        message_contains = $message:literal
    ) => {
        #[rstest]
        fn $test_name() -> std::io::Result<()> {
            assert_upload_fails_with_filesystem_error(
                $setup,
                UploadFailureExpectation {
                    container_id: $container_id,
                    copy_claude: $copy_claude,
                    copy_codex: $copy_codex,
                    expected_message_substring: $message,
                },
            )
        }
    };
}

filesystem_error_test!(
    upload_credentials_errors_when_selected_source_is_not_directory,
    |host_home| {
        write_file(&host_home.join(".claude"), "not-a-directory")?;
        Ok(host_home.to_path_buf())
    },
    container_id = "container-invalid",
    copy_claude = true,
    copy_codex = false,
    message_contains = "exists but is not a directory"
);

filesystem_error_test!(
    upload_credentials_errors_when_host_home_directory_cannot_be_opened,
    |host_home| Ok(host_home.join("missing-home-directory")),
    container_id = "container-missing-home",
    copy_claude = true,
    copy_codex = true,
    message_contains = "failed to open host home directory"
);

filesystem_error_test!(
    upload_credentials_errors_when_host_home_path_is_not_directory,
    |host_home| {
        let host_home_file = host_home.join("host-home-file");
        write_file(&host_home_file, "not-a-directory")?;
        Ok(host_home_file)
    },
    container_id = "container-home-file",
    copy_claude = true,
    copy_codex = true,
    message_contains = "failed to open host home directory"
);
