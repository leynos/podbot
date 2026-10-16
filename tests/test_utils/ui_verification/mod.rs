//! Synchronous MSRV/LLVM routing for the two nested Cargo UI verification suites.

use std::{ffi::OsString, process::Command};

use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;

/// Pure verification flag selection without ambient environment access.
mod flags;

/// Compatibility selection and scoped lifetime contracts.
#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;

/// Exact selector and real offline provisioning contracts.
#[cfg(test)]
mod toolchain_tests;

/// Public profile selection and composed lifecycle contracts.
#[cfg(test)]
mod boundary_tests;

/// Errors selecting a provisioned, compatible nested Cargo verification route.
#[derive(Debug, Error)]
pub enum VerificationError {
    /// An environment input could not be represented safely as Unicode.
    #[error("UI verification requires a Unicode {0} value")]
    NonUnicode(&'static str),
    /// Caller flags are outside the explicitly supported verification contract.
    #[error("unsupported UI verification flags: {0}; use the LLVM compatibility route")]
    UnsupportedFlags(String),
    /// A Cargo profile explicitly selects an incompatible backend.
    #[error("UI verification rejects non-LLVM profile backend override {0}")]
    ProfileBackend(String),
    /// An unqualified workspace wrapper could change compiler selection.
    #[error(
        "UI verification cannot prove an injected RUSTC_WORKSPACE_WRAPPER route; use the qualified coverage/admission wrapper route"
    )]
    WorkspaceWrapper,
    /// Rustup could not resolve the compatibility Cargo executable.
    #[error(
        "cannot resolve compatibility tooling: {0}; run rustup toolchain install {toolchain}",
        toolchain = compatibility_toolchain(env!("CARGO_PKG_RUST_VERSION"))
    )]
    Provisioning(String),
}

/// Resolve an installed compatibility Cargo and run complete fixture registration.
///
/// `TestCases` is constructed and dropped inside the scoped environment: its
/// destructor launches the actual Cargo work. Only the two UI suite consumers
/// should call this boundary; registration must stay synchronous.
///
/// # Errors
///
/// Returns an error for missing compatibility tooling or unsupported flags.
///
/// # Panics
///
/// Propagates registration or trybuild failures after restoring the environment.
///
/// # Examples
///
/// ```ignore
/// run_cases(|cases| cases.pass("tests/ui/stable_exec_context_signatures.rs"))?;
/// ```
pub fn run_cases(register: impl FnOnce(&trybuild::TestCases)) -> Result<(), VerificationError> {
    // Capture caller inputs under temp-env's same lock used for the later scope.
    temp_env::with_vars([] as [(&str, Option<&str>); 0], || {
        let route = VerificationRoute::from_environment()?;
        let executables = resolve_executables(env!("CARGO_PKG_RUST_VERSION"))?;
        route.run_registration(&executables, trybuild::TestCases::new, register);
        Ok(())
    })
}

/// Explicit inputs retained across a single synchronous verification lifetime.
struct VerificationRoute {
    /// LLVM-compatible unit-separated flags, including trybuild's own flags.
    encoded_flags: String,
}

impl VerificationRoute {
    /// Read ambient flags once at the integration boundary.
    fn from_environment() -> Result<Self, VerificationError> {
        reject_profile_backends(std::env::vars_os())?;
        if std::env::var_os("RUSTC_WORKSPACE_WRAPPER").is_some_and(|value| !value.is_empty()) {
            return Err(VerificationError::WorkspaceWrapper);
        }
        let encoded = unicode_variable("CARGO_ENCODED_RUSTFLAGS")?;
        let ordinary = unicode_variable("RUSTFLAGS")?;
        Ok(Self {
            encoded_flags: flags::verification_flags(encoded.as_deref(), ordinary.as_deref())?,
        })
    }

    /// Compose construction, registration and destruction within the UI scope.
    ///
    /// Only `run_cases` and its sibling lifecycle contracts use this seam; the
    /// factory must not return cases that escape this synchronous lifetime.
    fn run_registration<T>(
        &self,
        executables: &CompatibilityExecutables,
        create: impl FnOnce() -> T,
        register: impl FnOnce(&T),
    ) {
        self.run(executables, || {
            let cases = create();
            register(&cases);
            drop(cases);
        });
    }

    /// Retain wrapper, profiling and target context while selecting the UI route.
    fn run(&self, executables: &CompatibilityExecutables, body: impl FnOnce()) {
        temp_env::with_vars(
            [
                ("CARGO_ENCODED_RUSTFLAGS", Some(self.encoded_flags.as_str())),
                ("RUSTUP_TOOLCHAIN", Some(executables.toolchain.as_str())),
                ("CARGO", Some(executables.cargo.as_str())),
                ("RUSTC", Some(executables.rustc.as_str())),
            ],
            body,
        );
    }
}

/// Read one Unicode environment input without silently replacing invalid data.
fn unicode_variable(name: &'static str) -> Result<Option<String>, VerificationError> {
    std::env::var_os(name)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| VerificationError::NonUnicode(name))
        })
        .transpose()
}

/// Refuse profile overrides that encoded rustflags cannot reliably neutralize.
fn reject_profile_backends(
    inputs: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<(), VerificationError> {
    for (key, value) in inputs {
        let Some(name) = key.to_str() else { continue };
        if is_profile_backend_key(name) && value != "llvm" {
            return Err(VerificationError::ProfileBackend(name.to_owned()));
        }
    }
    Ok(())
}

/// Recognize only profile backend keys at the private UI environment boundary.
///
/// For example, `CARGO_PROFILE_DEV_CODEGEN_BACKEND` matches, whereas
/// `CARGO_PROFILE_DEV_OPT_LEVEL` and `OTHER_CODEGEN_BACKEND` do not.
fn is_profile_backend_key(name: &str) -> bool {
    name.starts_with("CARGO_PROFILE_") && name.ends_with("_CODEGEN_BACKEND")
}

/// Executables resolved from the same provisioned compatibility toolchain.
struct CompatibilityExecutables {
    /// Concrete minimum version shared by lookup and the nested environment.
    toolchain: String,
    /// Cargo must not inherit an absolute development-toolchain override.
    cargo: Utf8PathBuf,
    /// Explicit rustc selection retains existing coverage/admission wrappers.
    rustc: Utf8PathBuf,
}

/// Resolve both executables, then verify Cargo without invoking rustc directly.
fn resolve_executables(msrv: &str) -> Result<CompatibilityExecutables, VerificationError> {
    let toolchain = compatibility_toolchain(msrv);
    let cargo = resolve_executable(&toolchain, "cargo")?;
    let rustc = resolve_executable(&toolchain, "rustc")?;
    if cargo.parent() != rustc.parent() {
        return Err(VerificationError::Provisioning(
            "Cargo and rustc resolve to different toolchains".into(),
        ));
    }
    let version = Command::new(cargo.as_str())
        .arg("--version")
        .env("RUSTUP_TOOLCHAIN", &toolchain)
        .output()
        .map_err(|error| VerificationError::Provisioning(error.to_string()))?;
    validate_version(version.status.success(), &version.stdout, &toolchain)?;
    Ok(CompatibilityExecutables {
        toolchain,
        cargo,
        rustc,
    })
}

/// Convert the declared MSRV to its exact minimum Rustup selector.
///
/// Only this UI boundary owns the conversion: `1.88` selects `1.88.0`, while
/// `1.88.1` remains exact. This avoids refreshing a partial tracking channel.
fn compatibility_toolchain(msrv: &str) -> String {
    if msrv.split('.').count() == 2 {
        format!("{msrv}.0")
    } else {
        msrv.to_owned()
    }
}

/// Resolve one trusted Rustup toolchain executable without executing a compiler.
fn resolve_executable(toolchain: &str, name: &str) -> Result<Utf8PathBuf, VerificationError> {
    let resolution = Command::new("rustup")
        .args(["which", "--toolchain", toolchain, name])
        .output()
        .map_err(|error| VerificationError::Provisioning(error.to_string()))?;
    validate_executable_path(resolution.status.success(), &resolution.stdout, name)
}

/// Require the expected executable basename after lossless absolute-path parsing.
fn validate_executable_path(
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
fn resolved_path(success: bool, output: &[u8]) -> Result<Utf8PathBuf, VerificationError> {
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
fn validate_version(
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
