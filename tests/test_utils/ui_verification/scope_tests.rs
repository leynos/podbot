//! Contracts for compatibility selection and the child-process route.

use std::{
    collections::HashMap,
    env::VarError,
    ffi::{OsStr, OsString},
};

use mockable::MockEnv;
use rstest::rstest;

use super::{
    CHILD_MARKER, CompatibilityExecutables, VerificationError, VerificationRoute, libtest_name,
    reject_profile_backends, resolved_path, validate_executable_path, validate_version,
};

/// Host-native absolute executable spelling used only by resolver contracts.
const ABSOLUTE_CARGO_PATH: &[u8] = if cfg!(windows) {
    b"C:\\provisioned\\bin\\cargo.exe\n"
} else {
    b"/provisioned/bin/cargo\n"
};

/// Coverage instrumentation context key, shared only by these scope contracts.
const COVERAGE_FLAGS_KEY: &str = "__CARGO_LLVM_COV_RUSTC_WRAPPER_RUSTFLAGS";

/// Original admission wrapper key, shared only by these scope contracts.
const ADMISSION_WRAPPER_KEY: &str = "__CARGO_LLVM_COV_RUSTC_WRAPPER_PRE_EXISTING";

/// Inherited context the child route must neither override nor remove.
const INHERITED_CONTEXT_KEYS: [&str; 5] = [
    "RUSTC_WRAPPER",
    COVERAGE_FLAGS_KEY,
    "__CARGO_LLVM_COV_RUSTC_WRAPPER",
    ADMISSION_WRAPPER_KEY,
    "LLVM_PROFILE_FILE",
];

/// Construct fixed executable paths only for the UI scope and lifecycle tests.
///
/// These paths are test data, not provisioned compiler lookup; callers retain
/// all observations and assertions in their recognized test bodies.
pub(super) fn compatibility_executables() -> CompatibilityExecutables {
    CompatibilityExecutables {
        toolchain: "1.88.0".into(),
        cargo: "/compatibility/cargo".into(),
        rustc: "/compatibility/rustc".into(),
    }
}

/// Build an injected reader over fixed variables, with every other key absent.
///
/// A value that is not valid Unicode is reported as `VarError::NotUnicode`.
pub(super) fn reader_with(variables: Vec<(&'static str, OsString)>) -> MockEnv {
    let mut reader = MockEnv::new();
    reader.expect_raw().returning(move |key| {
        match variables.iter().find(|(name, _)| *name == key) {
            None => Err(VarError::NotPresent),
            Some((_, value)) => value
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| VarError::NotUnicode(value.clone())),
        }
    });
    reader
}

/// Construct a route carrying already-encoded verification flags.
fn route_with_flags(flags: &str) -> VerificationRoute {
    VerificationRoute {
        encoded_flags: flags.to_owned(),
    }
}

#[test]
fn child_command_selects_the_route_and_leaves_inherited_context_intact() {
    let route = route_with_flags("verification-flags");
    let command = route
        .child_command(&compatibility_executables(), "crate_name::module::a_test")
        .expect("the test binary path should resolve");
    let set: HashMap<&OsStr, Option<&OsStr>> = command.get_envs().collect();
    for (key, expected) in [
        ("CARGO_ENCODED_RUSTFLAGS", "verification-flags"),
        ("RUSTUP_TOOLCHAIN", "1.88.0"),
        ("CARGO", "/compatibility/cargo"),
        ("RUSTC", "/compatibility/rustc"),
        (CHILD_MARKER, "1"),
    ] {
        assert_eq!(
            set.get(OsStr::new(key)).copied().flatten(),
            Some(OsStr::new(expected)),
            "the child must be given {key}"
        );
    }
    for key in INHERITED_CONTEXT_KEYS {
        assert!(
            !set.contains_key(OsStr::new(key)),
            "{key} must be inherited unchanged, neither overridden nor removed"
        );
    }
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        ["--exact", "module::a_test", "--nocapture"],
        "the child must run the calling test alone"
    );
}

#[test]
fn libtest_name_drops_the_crate_segment() {
    assert_eq!(
        libtest_name("compile_contract::a_test"),
        "a_test",
        "the binary's own crate segment is not part of the libtest name"
    );
    assert_eq!(
        libtest_name("bare"),
        "bare",
        "a name without a crate segment is used as given"
    );
}

#[rstest]
#[case::llvm("llvm", true)]
#[case::cranelift("cranelift", false)]
#[case::empty("", false)]
fn profile_backend_is_checked_independently_of_encoded_flags(
    #[case] backend: &str,
    #[case] accepted: bool,
) {
    let result = reject_profile_backends([(
        OsString::from("CARGO_PROFILE_DEV_CODEGEN_BACKEND"),
        OsString::from(backend),
    )]);
    assert_eq!(
        result.is_ok(),
        accepted,
        "profile backend exclusion must be explicit"
    );
}

#[rstest]
#[case::prefix_only("CARGO_PROFILE_DEV_OPT_LEVEL", false)]
#[case::suffix_only("OTHER_CODEGEN_BACKEND", false)]
#[case::wrong_case("cargo_profile_dev_codegen_backend", false)]
#[case::exact_test_profile("CARGO_PROFILE_TEST_CODEGEN_BACKEND", true)]
#[case::empty_profile_segment("CARGO_PROFILE__CODEGEN_BACKEND", true)]
fn profile_backend_key_shape_is_preserved(#[case] name: &str, #[case] rejected: bool) {
    let result = reject_profile_backends([(OsString::from(name), OsString::from("cranelift"))]);
    assert_eq!(
        result.is_err(),
        rejected,
        "only exact prefix and suffix matches select backend policy"
    );
}

#[rstest]
#[case::current(true, b"cargo 1.88.0 (873a06493 2025-05-10)\n".as_slice(), true)]
#[case::nightly(true, b"cargo 1.88.0-nightly (abcdef 2025-05-10)\n".as_slice(), false)]
#[case::newer(true, b"cargo 1.99.0\n".as_slice(), false)]
#[case::different_patch(true, b"cargo 1.88.1\n".as_slice(), false)]
#[case::failed(false, b"cargo 1.88.0\n".as_slice(), false)]
fn cargo_resolution_requires_the_compatibility_version(
    #[case] success: bool,
    #[case] output: &[u8],
    #[case] accepted: bool,
) {
    assert_eq!(
        validate_version(success, output, "1.88.0").is_ok(),
        accepted,
        "wrong Cargo must fail before registration"
    );
}

#[rstest]
#[case::absolute(true, ABSOLUTE_CARGO_PATH, true)]
#[case::relative(true, b"cargo\n".as_slice(), false)]
#[case::multiple(true, b"/bin/cargo\n/bin/other\n".as_slice(), false)]
#[case::missing(false, b"/bin/cargo\n".as_slice(), false)]
#[case::non_unicode(true, b"\xff".as_slice(), false)]
fn failed_or_ambiguous_provisioning_is_refused(
    #[case] success: bool,
    #[case] output: &[u8],
    #[case] accepted: bool,
) {
    assert_eq!(
        resolved_path(success, output).is_ok(),
        accepted,
        "Cargo resolution must be unambiguous"
    );
}

/// Resolve a route from an injected reader holding only
/// `CARGO_ENCODED_RUSTFLAGS` set to `flags`, with no profile-backend inputs.
///
/// No workspace wrapper is present, so the outcome depends on the flags alone.
fn route_from_encoded_flags(
    flags: impl Into<OsString>,
) -> Result<VerificationRoute, VerificationError> {
    let reader = reader_with(vec![("CARGO_ENCODED_RUSTFLAGS", flags.into())]);
    VerificationRoute::from_environment(&reader, [])
}

#[test]
fn unsupported_caller_flags_fail_before_registration() {
    assert!(
        matches!(
            route_from_encoded_flags("--unknown-ui-route"),
            Err(VerificationError::UnsupportedFlags(_))
        ),
        "unrecognized input must not be silently discarded"
    );
}

#[cfg(unix)]
#[test]
fn non_unicode_flags_are_rejected_without_lossy_conversion() {
    use std::os::unix::ffi::OsStringExt;

    let non_unicode = OsString::from_vec(vec![0xff]);
    assert!(
        matches!(
            route_from_encoded_flags(non_unicode),
            Err(VerificationError::NonUnicode("CARGO_ENCODED_RUSTFLAGS"))
        ),
        "invalid Unicode must not silently disappear"
    );
}

#[test]
fn unknown_workspace_wrapper_is_refused_without_silently_bypassing_it() {
    let reader = reader_with(vec![(
        "RUSTC_WORKSPACE_WRAPPER",
        OsString::from("unqualified-workspace-wrapper"),
    )]);
    assert!(
        matches!(
            VerificationRoute::from_environment(&reader, []),
            Err(VerificationError::WorkspaceWrapper)
        ),
        "an unknown wrapper must not defeat compiler selection"
    );
}

#[test]
fn rustup_must_resolve_the_requested_executable_instead_of_an_arbitrary_path() {
    assert!(
        validate_executable_path(true, ABSOLUTE_CARGO_PATH, "cargo").is_ok(),
        "host-native Cargo resolution should be accepted"
    );
    assert!(
        validate_executable_path(true, ABSOLUTE_CARGO_PATH, "rustc").is_err(),
        "a Cargo path must not masquerade as rustc"
    );
}

/// Decoding precedes process-status and shape errors at the resolver boundary.
#[rstest]
#[case::failed_non_unicode(false, b"\xff".as_slice(), "non-Unicode Cargo path")]
#[case::successful_non_unicode(true, b"\xff".as_slice(), "non-Unicode Cargo path")]
#[case::failed_absolute(
    false,
    ABSOLUTE_CARGO_PATH,
    "Rustup did not return an absolute Cargo path"
)]
#[case::relative(true, b"cargo\n".as_slice(), "Rustup did not return an absolute Cargo path")]
#[case::multiline(true, b"/bin/cargo\n/bin/other\n".as_slice(), "Rustup did not return an absolute Cargo path")]
fn resolver_error_precedence_is_preserved(
    #[case] success: bool,
    #[case] output: &[u8],
    #[case] expected: &str,
) {
    let error = resolved_path(success, output).expect_err("invalid resolution must fail");
    assert!(
        matches!(error, VerificationError::Provisioning(detail) if detail == expected),
        "invalid UTF-8 must retain precedence over status and path-shape errors"
    );
}
