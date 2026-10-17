//! Public profile selection and composed create/register/drop UI contracts.

use std::{
    cell::{Cell, RefCell},
    env::VarError,
    ffi::OsString,
};

use mockable::{DefaultEnv, Env, MockEnv};
use rstest::rstest;

use super::{
    VerificationError, VerificationRoute, has_workspace_wrapper,
    tests::{DropObservation, compatibility_executables, with_isolated_profile_backends},
    unicode_variable,
};

/// Build the native payload shared only by the fixed-key reader policy tables.
#[cfg(unix)]
fn non_unicode_payload() -> OsString {
    use std::os::unix::ffi::OsStringExt;

    OsString::from_vec(vec![0xff])
}

/// Build the native payload shared only by the fixed-key reader policy tables.
#[cfg(windows)]
fn non_unicode_payload() -> OsString {
    use std::os::windows::ffi::OsStringExt;

    OsString::from_wide(&[0xd800])
}

/// Unicode flag reads preserve absence, empty values, content and error names.
#[rstest]
#[case::encoded_absent("CARGO_ENCODED_RUSTFLAGS", Err(VarError::NotPresent), None, false)]
#[case::encoded_empty("CARGO_ENCODED_RUSTFLAGS", Ok(String::new()), Some(""), false)]
#[case::encoded_value(
    "CARGO_ENCODED_RUSTFLAGS",
    Ok(String::from("-Copt-level=1")),
    Some("-Copt-level=1"),
    false
)]
#[cfg_attr(
    unix,
    case::encoded_unix_non_unicode(
        "CARGO_ENCODED_RUSTFLAGS",
        Err(VarError::NotUnicode(non_unicode_payload())),
        None,
        true
    )
)]
#[cfg_attr(
    windows,
    case::encoded_windows_non_unicode(
        "CARGO_ENCODED_RUSTFLAGS",
        Err(VarError::NotUnicode(non_unicode_payload())),
        None,
        true
    )
)]
#[case::ordinary_absent("RUSTFLAGS", Err(VarError::NotPresent), None, false)]
#[case::ordinary_empty("RUSTFLAGS", Ok(String::new()), Some(""), false)]
#[case::ordinary_value(
    "RUSTFLAGS",
    Ok(String::from("-Copt-level=1")),
    Some("-Copt-level=1"),
    false
)]
#[cfg_attr(
    unix,
    case::ordinary_unix_non_unicode(
        "RUSTFLAGS",
        Err(VarError::NotUnicode(non_unicode_payload())),
        None,
        true
    )
)]
#[cfg_attr(
    windows,
    case::ordinary_windows_non_unicode(
        "RUSTFLAGS",
        Err(VarError::NotUnicode(non_unicode_payload())),
        None,
        true
    )
)]
fn unicode_variable_preserves_fixed_key_policy(
    #[case] name: &'static str,
    #[case] raw_value: Result<String, VarError>,
    #[case] expected_value: Option<&str>,
    #[case] expects_non_unicode: bool,
) {
    let mut environment_reader = MockEnv::new();
    environment_reader
        .expect_raw()
        .withf(move |actual_key: &str| actual_key == name)
        .times(1)
        .return_once(move |_| raw_value);

    let result = unicode_variable(&environment_reader, name);
    if expects_non_unicode {
        assert!(
            matches!(result, Err(VerificationError::NonUnicode(actual_name)) if actual_name == name),
            "invalid Unicode must retain the exact fixed input name"
        );
    } else {
        assert_eq!(
            result
                .expect("present and absent Unicode inputs should be accepted")
                .as_deref(),
            expected_value,
            "the reader must preserve the observed Unicode value"
        );
    }
}

/// Cargo's workspace wrapper policy uses native payload emptiness on errors.
#[rstest]
#[case::absent(Err(VarError::NotPresent), false)]
#[case::empty(Ok(String::new()), false)]
#[case::unicode_value(Ok(String::from("wrapper")), true)]
#[case::non_unicode_empty(Err(VarError::NotUnicode(OsString::new())), false)]
#[cfg_attr(
    unix,
    case::unix_non_unicode(Err(VarError::NotUnicode(non_unicode_payload())), true)
)]
#[cfg_attr(
    windows,
    case::windows_non_unicode(Err(VarError::NotUnicode(non_unicode_payload())), true)
)]
fn workspace_wrapper_policy_checks_the_fixed_key_and_native_payload(
    #[case] raw_value: Result<String, VarError>,
    #[case] expected: bool,
) {
    let mut environment_reader = MockEnv::new();
    environment_reader
        .expect_raw()
        .withf(|actual_key: &str| actual_key == "RUSTC_WORKSPACE_WRAPPER")
        .times(1)
        .return_once(move |_| raw_value);

    assert_eq!(has_workspace_wrapper(&environment_reader), expected);
}

/// Profile policy is enforced at the environment-reading boundary, before flags.
#[rstest]
#[case::development("CARGO_PROFILE_DEV_CODEGEN_BACKEND", "cranelift")]
#[case::tests("CARGO_PROFILE_TEST_CODEGEN_BACKEND", "cranelift")]
#[case::custom("CARGO_PROFILE_CUSTOM_CODEGEN_BACKEND", "cranelift")]
#[case::empty_segment("CARGO_PROFILE__CODEGEN_BACKEND", "cranelift")]
#[case::compatible_llvm("CARGO_PROFILE_CUSTOM_CODEGEN_BACKEND", "llvm")]
fn environment_boundary_enforces_profile_backend(#[case] key: &str, #[case] value: &str) {
    with_isolated_profile_backends(|| {
        temp_env::with_vars(
            [
                (key, Some(value)),
                ("RUSTC_WORKSPACE_WRAPPER", None),
                ("CARGO_ENCODED_RUSTFLAGS", Some("")),
                ("RUSTFLAGS", None),
            ],
            || {
                let reader = DefaultEnv::new();
                let result = VerificationRoute::from_environment(&reader);
                if value == "llvm" {
                    assert!(
                        result.is_ok(),
                        "compatible LLVM must pass environment selection"
                    );
                } else {
                    assert!(
                        matches!(result, Err(VerificationError::ProfileBackend(ref actual)) if actual == key),
                        "the boundary must identify the exact incompatible profile key"
                    );
                }
                assert_eq!(
                    reader.raw(key).expect("the scoped backend remains present"),
                    value,
                    "selection must inspect without rewriting the backend value"
                );
            },
        );
    });
}

/// The composition seam keeps every lifecycle phase inside the selected scope.
#[test]
fn registration_constructs_registers_and_drops_before_environment_restoration() {
    temp_env::with_vars([("RUSTUP_TOOLCHAIN", Some("nightly-parent"))], || {
        let reader = DefaultEnv::new();
        let created = Cell::new(false);
        let registered = Cell::new(false);
        let dropped = RefCell::new(None);
        let route = VerificationRoute {
            encoded_flags: String::new(),
        };
        route.run_registration(
            &compatibility_executables(),
            || {
                created.set(reader.raw("RUSTUP_TOOLCHAIN").ok().as_deref() == Some("1.88.0"));
                DropObservation {
                    observed: &dropped,
                    reader: &reader,
                }
            },
            |_| {
                registered.set(reader.raw("RUSTUP_TOOLCHAIN").ok().as_deref() == Some("1.88.0"));
            },
        );
        assert!(
            created.get(),
            "construction must see the exact compatibility scope"
        );
        assert!(
            registered.get(),
            "registration must see the exact compatibility scope"
        );
        assert_eq!(
            dropped.borrow().as_deref(),
            Some("1.88.0"),
            "destruction must precede environment restoration"
        );
        assert_eq!(
            reader
                .raw("RUSTUP_TOOLCHAIN")
                .expect("parent selector restored"),
            "nightly-parent",
            "the parent selector must be restored after the composed lifetime"
        );
    });
}
