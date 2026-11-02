//! Contracts for compatibility selection and synchronous environment lifetimes.

use std::{
    cell::RefCell,
    ffi::OsString,
    panic::{AssertUnwindSafe, catch_unwind},
};

use mockable::{DefaultEnv, Env};
use rstest::rstest;

use super::{
    CompatibilityExecutables, VerificationError, VerificationRoute, reject_profile_backends,
    resolved_path, validate_executable_path, validate_version,
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

/// Fixed parent environment used only by the scoped restoration contract.
///
/// Keeping this test data separate leaves every observation and assertion in
/// the recognized test body; coverage and admission wrapper inputs stay intact.
const PARENT_ENVIRONMENT: [(&str, Option<&str>); 9] = [
    ("CARGO", Some("/parent/nightly/cargo")),
    ("RUSTC", Some("/parent/nightly/rustc")),
    ("RUSTUP_TOOLCHAIN", Some("nightly-parent")),
    ("CARGO_ENCODED_RUSTFLAGS", Some("parent-flags")),
    ("RUSTC_WRAPPER", Some("coverage-wrapper")),
    (COVERAGE_FLAGS_KEY, Some("instrumentation-context")),
    ("__CARGO_LLVM_COV_RUSTC_WRAPPER", Some("1")),
    (ADMISSION_WRAPPER_KEY, Some("admission-wrapper")),
    ("LLVM_PROFILE_FILE", Some("preserved-%p.profraw")),
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

/// Observe the live route during destruction without caching its selector.
pub(super) struct DropObservation<'a> {
    /// Toolchain selector present when this value was destroyed.
    pub(super) observed: &'a RefCell<Option<String>>,
    /// Live reader used to observe the selector at destruction time.
    pub(super) reader: &'a dyn Env,
}

impl Drop for DropObservation<'_> {
    fn drop(&mut self) {
        *self.observed.borrow_mut() = self.reader.raw("RUSTUP_TOOLCHAIN").ok();
    }
}

#[test]
fn verification_scope_preserves_wrappers_and_restores_the_parent_after_drop() {
    temp_env::with_vars(PARENT_ENVIRONMENT, || {
        let reader = DefaultEnv::new();
        let observed = RefCell::new(None);
        let route = VerificationRoute {
            encoded_flags: String::from("verification-flags"),
        };
        route.run(&compatibility_executables(), || {
            let _drop_observation = DropObservation {
                observed: &observed,
                reader: &reader,
            };
            assert_eq!(
                reader.raw("RUSTC").expect("scoped rustc exists"),
                "/compatibility/rustc",
                "absolute parent compiler must not leak"
            );
            assert_eq!(
                reader.raw("CARGO").expect("scoped Cargo exists"),
                "/compatibility/cargo",
                "the compatibility Cargo override must be active"
            );
            assert_eq!(
                reader.raw("CARGO_ENCODED_RUSTFLAGS").as_deref(),
                Ok("verification-flags"),
                "encoded verification flags must be active"
            );
            assert_eq!(
                reader.raw("RUSTC_WRAPPER").expect("wrapper exists"),
                "coverage-wrapper",
                "coverage wrapper selection must be preserved"
            );
            assert_eq!(
                reader.raw(COVERAGE_FLAGS_KEY).expect("context exists"),
                "instrumentation-context",
                "coverage instrumentation context must be preserved"
            );
            assert_eq!(
                reader.raw(ADMISSION_WRAPPER_KEY).expect("admission exists"),
                "admission-wrapper",
                "compiler admission wrapper must be preserved"
            );
            assert_eq!(
                reader.raw("LLVM_PROFILE_FILE").expect("profiling exists"),
                "preserved-%p.profraw",
                "coverage profile path must be preserved"
            );
        });
        assert_eq!(observed.borrow().as_deref(), Some("1.88.0"));
        assert_eq!(
            reader.raw("RUSTC").expect("parent compiler restored"),
            "/parent/nightly/rustc",
            "parent compiler selection must be restored"
        );
        assert_eq!(
            reader.raw("CARGO").expect("parent Cargo restored"),
            "/parent/nightly/cargo",
            "parent Cargo selection must be restored"
        );
        assert_eq!(
            reader.raw("RUSTUP_TOOLCHAIN").as_deref(),
            Ok("nightly-parent"),
            "parent toolchain selection must be restored"
        );
        assert_eq!(
            reader.raw("CARGO_ENCODED_RUSTFLAGS").as_deref(),
            Ok("parent-flags"),
            "parent encoded flags must be restored"
        );
    });
}

#[test]
fn verification_scope_restores_the_parent_when_registration_panics() {
    temp_env::with_vars([("RUSTUP_TOOLCHAIN", Some("nightly-parent"))], || {
        let reader = DefaultEnv::new();
        let observed = RefCell::new(None);
        let route = VerificationRoute {
            encoded_flags: String::new(),
        };
        let panic_result = catch_unwind(AssertUnwindSafe(|| {
            route.run(&compatibility_executables(), || {
                let _drop_observation = DropObservation {
                    observed: &observed,
                    reader: &reader,
                };
                panic!("intentional registration failure");
            });
        }));
        assert!(
            panic_result.is_err(),
            "the registration panic must propagate"
        );
        assert_eq!(
            observed.borrow().as_deref(),
            Some("1.88.0"),
            "unwinding destruction must see compatibility selection"
        );
        assert_eq!(
            reader
                .raw("RUSTUP_TOOLCHAIN")
                .expect("parent toolchain restored"),
            "nightly-parent",
            "parent toolchain selection must be restored"
        );
    });
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

#[test]
fn unsupported_caller_flags_fail_before_registration() {
    with_isolated_profile_backends(|| {
        temp_env::with_vars(
            [
                ("CARGO_ENCODED_RUSTFLAGS", Some("--unknown-ui-route")),
                ("RUSTFLAGS", None),
                ("RUSTC_WORKSPACE_WRAPPER", None),
            ],
            || {
                let reader = DefaultEnv::new();
                assert!(
                    matches!(
                        VerificationRoute::from_environment(&reader),
                        Err(VerificationError::UnsupportedFlags(_))
                    ),
                    "unrecognized input must not be silently discarded"
                );
            },
        );
    });
}

#[cfg(unix)]
#[test]
fn non_unicode_flags_are_rejected_without_lossy_conversion() {
    use std::os::unix::ffi::OsStringExt;

    let non_unicode = OsString::from_vec(vec![0xff]);
    with_isolated_profile_backends(|| {
        temp_env::with_vars(
            [
                ("CARGO_ENCODED_RUSTFLAGS", Some(non_unicode)),
                ("RUSTFLAGS", None),
                ("RUSTC_WORKSPACE_WRAPPER", None),
            ],
            || {
                let reader = DefaultEnv::new();
                assert!(
                    matches!(
                        VerificationRoute::from_environment(&reader),
                        Err(VerificationError::NonUnicode("CARGO_ENCODED_RUSTFLAGS"))
                    ),
                    "invalid Unicode must not silently disappear"
                );
            },
        );
    });
}

/// Clear all profile-backend inputs only while exercising environment selection.
///
/// Only sibling UI scope/boundary tests use this fixture. It snapshots keys under
/// temp-env's shared lock, including custom profiles, and restores them on unwind.
pub(super) fn with_isolated_profile_backends(body: impl FnOnce()) {
    temp_env::with_vars([] as [(&str, Option<&str>); 0], || {
        let overrides = std::env::vars_os()
            .filter(|(key, _)| key.to_str().is_some_and(super::is_profile_backend_key))
            .map(|(key, _)| (key, None::<OsString>))
            .collect::<Vec<_>>();
        temp_env::with_vars(overrides, body);
    });
}

#[rstest]
#[case::development("CARGO_PROFILE_DEV_CODEGEN_BACKEND")]
#[case::tests("CARGO_PROFILE_TEST_CODEGEN_BACKEND")]
#[case::custom("CARGO_PROFILE_CUSTOM_CODEGEN_BACKEND")]
#[case::empty_segment("CARGO_PROFILE__CODEGEN_BACKEND")]
fn unknown_workspace_wrapper_is_refused_without_silently_bypassing_it(#[case] backend_key: &str) {
    temp_env::with_vars(
        [
            (backend_key, Some("cranelift")),
            (
                "RUSTC_WORKSPACE_WRAPPER",
                Some("unqualified-workspace-wrapper"),
            ),
        ],
        || {
            let reader = DefaultEnv::new();
            with_isolated_profile_backends(|| {
                assert!(
                    matches!(
                        VerificationRoute::from_environment(&reader),
                        Err(VerificationError::WorkspaceWrapper)
                    ),
                    "an unknown wrapper must not defeat compiler selection"
                );
                assert_eq!(
                    reader
                        .raw("RUSTC_WORKSPACE_WRAPPER")
                        .expect("wrapper remains present"),
                    "unqualified-workspace-wrapper",
                    "refusal must preserve the caller's topology"
                );
            });
            assert_eq!(
                reader.raw(backend_key).expect("profile backend restored"),
                "cranelift",
                "profile isolation must restore every inherited matching key"
            );
        },
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
