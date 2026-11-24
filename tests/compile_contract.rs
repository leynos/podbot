//! Compile-pass checks for the library boundary and BDD step contracts.

#[path = "test_utils/ui_verification/mod.rs"]
mod ui_verification;

/// Register `register`'s fixtures for the calling test, named `test`.
///
/// `test` must be the calling test function's own name: the UI boundary
/// re-executes that test alone in a child process.
fn run_fixtures(test: &str, register: impl FnOnce(&trybuild::TestCases)) {
    ui_verification::run_cases(&format!("{}::{test}", module_path!()), register).expect(
        "UI verification requires the provisioned exact MSRV/LLVM route; see resolver error",
    );
}

#[test]
fn stable_exec_context_signatures_compile() {
    run_fixtures("stable_exec_context_signatures_compile", |test_cases| {
        test_cases.pass("tests/ui/stable_exec_context_signatures.rs");
        test_cases.pass("tests/ui/stable_run_request_signatures.rs");
        test_cases.pass("tests/ui/stable_embedder_uses_only_stable_modules.rs");
    });
}

#[test]
fn stable_repository_clone_signatures_compile() {
    run_fixtures("stable_repository_clone_signatures_compile", |test_cases| {
        test_cases.pass("tests/ui/stable_repository_clone_signatures.rs");
    });
}

#[test]
fn bdd_step_return_signatures_compile() {
    run_fixtures("bdd_step_return_signatures_compile", |test_cases| {
        test_cases.pass("tests/ui/bdd_step_return_signatures.rs");
    });
}

#[test]
#[cfg(feature = "experimental")]
fn experimental_run_agent_signature_compiles() {
    run_fixtures("experimental_run_agent_signature_compiles", |test_cases| {
        test_cases.pass("tests/ui/experimental_run_agent_signatures.rs");
    });
}

#[test]
#[cfg(feature = "internal")]
fn config_internal_reexport_is_available_with_internal() {
    run_fixtures(
        "config_internal_reexport_is_available_with_internal",
        |test_cases| {
            test_cases.pass("tests/ui/config_internal_reexport_available.rs");
        },
    );
}

#[test]
#[cfg(feature = "internal")]
fn exec_client_mock_contract_compiles() {
    run_fixtures("exec_client_mock_contract_compiles", |test_cases| {
        test_cases.pass("tests/ui/exec_client_mock_compiles.rs");
        test_cases.compile_fail("tests/ui/exec_client_mock_missing_step_result.rs");
        test_cases.compile_fail("tests/ui/exec_client_mock_malformed_invocation.rs");
    });
}

#[test]
#[cfg(not(feature = "internal"))]
fn config_internal_reexport_is_unavailable_without_internal() {
    run_fixtures(
        "config_internal_reexport_is_unavailable_without_internal",
        |test_cases| {
            test_cases.compile_fail("tests/ui/config_internal_reexport_unavailable.rs");
        },
    );
}

#[test]
#[cfg(not(any(feature = "cli", feature = "internal")))]
fn stable_api_without_cli_compiles() {
    run_fixtures("stable_api_without_cli_compiles", |test_cases| {
        test_cases.pass("tests/ui/stable_exec_context_no_cli.rs");
        test_cases.compile_fail("tests/ui/stable_embedder_cannot_use_engine_without_internal.rs");
    });
}

/// Printed by the registration callback once its scope assertion has held.
const CALLBACK_RAN_MARKER: &str = "UI_REGISTRATION_CALLBACK_RAN";

/// A valid fixture registered as compile-fail must fail during trybuild's drop.
///
/// The registration callback runs in the child, where it asserts the exact
/// compatibility selector and prints a marker; the parent's panic carries the
/// child's output, so the marker proves the callback ran and held before the
/// destructor failed.
#[test]
fn ui_registration_runs_and_propagates_destructor_failure() {
    use mockable::{DefaultEnv, Env};
    use std::{io::Write as _, panic::catch_unwind};

    let execution = catch_unwind(|| {
        run_fixtures(
            "ui_registration_runs_and_propagates_destructor_failure",
            |cases| {
                assert_eq!(
                    DefaultEnv
                        .raw("RUSTUP_TOOLCHAIN")
                        .expect("callback selector exists"),
                    "1.88.0",
                    "the real registration callback must run inside the exact scope"
                );
                writeln!(std::io::stdout(), "{CALLBACK_RAN_MARKER}")
                    .expect("the marker should reach the child's stdout");
                cases.compile_fail("tests/ui/stable_exec_context_signatures.rs");
            },
        );
    });
    let payload = execution
        .expect_err("a passing fixture registered as compile-fail must propagate its failure");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .expect("the parent panics with the child's output");
    assert!(
        message.contains(CALLBACK_RAN_MARKER),
        "the public boundary must invoke registration inside the exact scope: {message}"
    );
}
