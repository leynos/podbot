//! Synchronous MSRV/LLVM routing for the two nested Cargo UI verification suites.

use std::{
    ffi::OsString,
    io::{self, Write as _},
    process::Command,
};

use camino::{Utf8Path, Utf8PathBuf};
use mockable::{DefaultEnv, Env};
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
/// `trybuild` spawns Cargo with the environment it inherits and has no
/// injection point, and a test must not change its own process environment. The
/// parent therefore re-executes the calling test alone in a child copy of the
/// test binary, started with `Command::env` carrying the compatibility route;
/// the child constructs `TestCases`, registers the fixtures and drops the cases,
/// whose destructor launches the actual Cargo work. Only the two UI suite
/// consumers should call this boundary; registration must stay synchronous.
///
/// `qualified_test` is the calling test's `concat!(module_path!(), "::", name)`.
///
/// # Errors
///
/// Returns an error for missing compatibility tooling or unsupported flags.
///
/// # Panics
///
/// In the parent, panics with the child's output when the child fails; in the
/// child, a registration or trybuild failure ends the process with a failing
/// status after the panic message, so the failure reaches the parent even when
/// the calling test catches the unwind.
///
/// # Examples
///
/// ```ignore
/// run_cases(
///     concat!(module_path!(), "::stable_signatures_compile"),
///     |cases| cases.pass("tests/ui/stable_exec_context_signatures.rs"),
/// )?;
/// ```
pub fn run_cases(
    qualified_test: &str,
    register: impl FnOnce(&trybuild::TestCases),
) -> Result<(), VerificationError> {
    if running_in_child() {
        run_registration_in_child(trybuild::TestCases::new, register);
        return Ok(());
    }
    let route = VerificationRoute::from_environment(&DefaultEnv, std::env::vars_os())?;
    let executables = resolve_executables(env!("CARGO_PKG_RUST_VERSION"), &[])?;
    let mut child = route.child_command(&executables, qualified_test)?;
    let output = child
        .output()
        .map_err(|error| VerificationError::Provisioning(error.to_string()))?;
    replay_or_panic(qualified_test, &output);
    Ok(())
}

/// Replay a successful child's output, or panic carrying a failed child's.
///
/// # Panics
///
/// Panics with the child's status and output when the child failed.
fn replay_or_panic(qualified_test: &str, output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{qualified_test} failed in its UI verification child ({}):\n{}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Pass the child's progress output through, as an in-process run showed it.
    io::stdout()
        .write_all(&output.stdout)
        .expect("the child's output should replay to stdout");
    io::stderr()
        .write_all(&output.stderr)
        .expect("the child's output should replay to stderr");
}

/// Names the variable whose presence makes this binary run the fixtures itself.
const CHILD_MARKER: &str = "PODBOT_UI_VERIFICATION_CHILD";

/// Whether this process is the child started by `run_cases`.
///
/// A test whose assertions belong to the parent alone returns early when this
/// holds, so the child's outcome reflects only the registration and the drop.
pub fn running_in_child() -> bool {
    std::env::var_os(CHILD_MARKER).is_some()
}

/// Compose construction, registration and destruction inside the child.
///
/// A panic ends the child with a failing status: the calling test may catch the
/// unwind, and the parent must still see the failure. The factory must not
/// return cases that escape this synchronous lifetime.
fn run_registration_in_child<T>(create: impl FnOnce() -> T, register: impl FnOnce(&T)) {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_registration(create, register);
    }));
    if outcome.is_err() {
        std::process::exit(1);
    }
}

/// Construct, register and explicitly drop, in that order.
///
/// Only the child path and the sibling lifecycle contracts use this seam.
fn run_registration<T>(create: impl FnOnce() -> T, register: impl FnOnce(&T)) {
    let cases = create();
    register(&cases);
    drop(cases);
}

/// Explicit inputs retained across a single synchronous verification lifetime.
struct VerificationRoute {
    /// LLVM-compatible unit-separated flags, including trybuild's own flags.
    encoded_flags: String,
}

impl VerificationRoute {
    /// Read ambient flags once at the integration boundary.
    fn from_environment(
        environment_reader: &dyn Env,
        process_variables: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> Result<Self, VerificationError> {
        reject_profile_backends(process_variables)?;
        if has_workspace_wrapper(environment_reader) {
            return Err(VerificationError::WorkspaceWrapper);
        }
        let encoded = unicode_variable(environment_reader, "CARGO_ENCODED_RUSTFLAGS")?;
        let ordinary = unicode_variable(environment_reader, "RUSTFLAGS")?;
        Ok(Self {
            encoded_flags: flags::verification_flags(encoded.as_deref(), ordinary.as_deref())?,
        })
    }

    /// The variables the child sets to select the UI route.
    ///
    /// Every other inherited variable, including the coverage and admission
    /// wrappers, the profiling path and the target context, is left intact.
    fn route_variables<'a>(
        &'a self,
        executables: &'a CompatibilityExecutables,
    ) -> [(&'static str, &'a str); 4] {
        [
            ("CARGO_ENCODED_RUSTFLAGS", self.encoded_flags.as_str()),
            ("RUSTUP_TOOLCHAIN", executables.toolchain.as_str()),
            ("CARGO", executables.cargo.as_str()),
            ("RUSTC", executables.rustc.as_str()),
        ]
    }

    /// Build the command that re-runs `qualified_test` alone in this binary.
    fn child_command(
        &self,
        executables: &CompatibilityExecutables,
        qualified_test: &str,
    ) -> Result<Command, VerificationError> {
        let program = std::env::current_exe()
            .map_err(|error| VerificationError::Provisioning(error.to_string()))?;
        let mut command = Command::new(program);
        command
            .args(["--exact", libtest_name(qualified_test), "--nocapture"])
            .env(CHILD_MARKER, "1");
        for (key, value) in self.route_variables(executables) {
            command.env(key, value);
        }
        Ok(command)
    }
}

/// The libtest name of a test, given its `module_path!()`-qualified name: the
/// binary's own crate segment is not part of the name libtest filters on.
fn libtest_name(qualified: &str) -> &str {
    qualified
        .split_once("::")
        .map_or(qualified, |(_crate, rest)| rest)
}

/// Read one Unicode environment input without silently replacing invalid data.
fn unicode_variable(
    environment_reader: &dyn Env,
    name: &'static str,
) -> Result<Option<String>, VerificationError> {
    match environment_reader.raw(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(VerificationError::NonUnicode(name)),
    }
}

/// Preserve Cargo's wrapper rejection, including its native empty-value check.
fn has_workspace_wrapper(environment_reader: &dyn Env) -> bool {
    match environment_reader.raw("RUSTC_WORKSPACE_WRAPPER") {
        Ok(value) => !value.is_empty(),
        Err(std::env::VarError::NotPresent) => false,
        Err(std::env::VarError::NotUnicode(value)) => !value.is_empty(),
    }
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
///
/// `rustup_variables` are set on each Rustup and Cargo child only, so a caller
/// can constrain resolution without changing its own process environment.
fn resolve_executables(
    msrv: &str,
    rustup_variables: &[(&str, &str)],
) -> Result<CompatibilityExecutables, VerificationError> {
    let toolchain = compatibility_toolchain(msrv);
    let cargo = resolve_executable(&toolchain, "cargo", rustup_variables)?;
    let rustc = resolve_executable(&toolchain, "rustc", rustup_variables)?;
    if cargo.parent() != rustc.parent() {
        return Err(VerificationError::Provisioning(
            "Cargo and rustc resolve to different toolchains".into(),
        ));
    }
    let version = Command::new(cargo.as_str())
        .arg("--version")
        .envs(rustup_variables.iter().copied())
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
fn resolve_executable(
    toolchain: &str,
    name: &str,
    rustup_variables: &[(&str, &str)],
) -> Result<Utf8PathBuf, VerificationError> {
    let resolution = Command::new("rustup")
        .args(["which", "--toolchain", toolchain, name])
        .envs(rustup_variables.iter().copied())
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
