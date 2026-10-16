//! Public profile selection and composed create/register/drop UI contracts.

use std::cell::{Cell, RefCell};

use rstest::rstest;

use super::{
    VerificationError, VerificationRoute,
    tests::{DropObservation, compatibility_executables, with_isolated_profile_backends},
};

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
                let result = VerificationRoute::from_environment();
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
                    std::env::var(key).expect("the scoped backend remains present"),
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
        let created = Cell::new(false);
        let registered = Cell::new(false);
        let dropped = RefCell::new(None);
        let route = VerificationRoute {
            encoded_flags: String::new(),
        };
        route.run_registration(
            &compatibility_executables(),
            || {
                created.set(std::env::var("RUSTUP_TOOLCHAIN").ok().as_deref() == Some("1.88.0"));
                DropObservation { observed: &dropped }
            },
            |_| {
                registered.set(std::env::var("RUSTUP_TOOLCHAIN").ok().as_deref() == Some("1.88.0"));
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
            std::env::var("RUSTUP_TOOLCHAIN").expect("parent selector restored"),
            "nightly-parent",
            "the parent selector must be restored after the composed lifetime"
        );
    });
}
