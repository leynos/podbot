//! Pure checks on Rustup's and Cargo's answers during compatibility resolution.

use camino::{Utf8Path, Utf8PathBuf};

use super::VerificationError;

/// Require the expected executable basename after lossless absolute-path parsing.
pub(super) fn validate_executable_path(
    success: bool,
    output: &[u8],
    name: &str,
) -> Result<Utf8PathBuf, VerificationError> {
    let executable = resolved_path(success, output)?;
    let expected_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    if executable.file_name() != Some(expected_name.as_str()) {
        return Err(VerificationError::Provisioning(format!(
            "Rustup did not resolve the requested {name} executable"
        )));
    }
    Ok(executable)
}

/// Reject failed, relative or multiline executable resolutions.
pub(super) fn resolved_path(
    success: bool,
    output: &[u8],
) -> Result<Utf8PathBuf, VerificationError> {
    let text = std::str::from_utf8(output)
        .map_err(|_| VerificationError::Provisioning("non-Unicode Cargo path".into()))?;
    let path = Utf8PathBuf::from(text.trim());
    if !success || !is_single_absolute_path(&path) {
        return Err(VerificationError::Provisioning(
            "Rustup did not return an absolute Cargo path".into(),
        ));
    }
    Ok(path)
}

/// Accept only one absolute executable spelling after UTF-8 decoding and trimming.
fn is_single_absolute_path(path: &Utf8Path) -> bool {
    path.is_absolute() && !path.as_str().contains(['\n', '\r'])
}

/// Require the exact stable minimum selected for the compatibility route.
pub(super) fn validate_version(
    success: bool,
    output: &[u8],
    toolchain: &str,
) -> Result<(), VerificationError> {
    let version = std::str::from_utf8(output)
        .map_err(|_| VerificationError::Provisioning("non-Unicode Cargo version".into()))?;
    let compatible = version
        .strip_prefix("cargo ")
        .and_then(|value| value.split_whitespace().next())
        == Some(toolchain);
    if !success || !compatible {
        return Err(VerificationError::Provisioning(format!(
            "expected Cargo {toolchain}, got {}",
            version.trim()
        )));
    }
    Ok(())
}
