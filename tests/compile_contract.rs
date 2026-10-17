//! Compile-pass checks for the library boundary and BDD step contracts.

#[path = "test_utils/ui_verification/mod.rs"]
mod ui_verification;

#[test]
fn stable_exec_context_signatures_compile() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/stable_exec_context_signatures.rs");
        test_cases.pass("tests/ui/stable_run_request_signatures.rs");
        test_cases.pass("tests/ui/stable_embedder_uses_only_stable_modules.rs");
    })
    .expect("UI verification requires the provisioned exact MSRV/LLVM route; see resolver error");
}

#[test]
fn stable_repository_clone_signatures_compile() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/stable_repository_clone_signatures.rs");
    })
    .expect("UI verification requires the provisioned exact MSRV/LLVM route; see resolver error");
}

#[test]
fn bdd_step_return_signatures_compile() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/bdd_step_return_signatures.rs");
    })
    .expect("UI verification requires the provisioned exact MSRV/LLVM route; see resolver error");
}

#[test]
#[cfg(feature = "experimental")]
fn experimental_run_agent_signature_compiles() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/experimental_run_agent_signatures.rs");
    })
    .expect("UI verification requires the provisioned exact MSRV/LLVM route; see resolver error");
}

#[test]
#[cfg(feature = "internal")]
fn config_internal_reexport_is_available_with_internal() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/config_internal_reexport_available.rs");
    })
    .expect("UI verification requires the provisioned exact MSRV/LLVM route; see resolver error");
}

#[test]
#[cfg(feature = "internal")]
fn exec_client_mock_contract_compiles() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/exec_client_mock_compiles.rs");
        test_cases.compile_fail("tests/ui/exec_client_mock_missing_step_result.rs");
        test_cases.compile_fail("tests/ui/exec_client_mock_malformed_invocation.rs");
    })
    .expect("UI verification requires the provisioned exact MSRV/LLVM route; see resolver error");
}

#[test]
#[cfg(not(feature = "internal"))]
fn config_internal_reexport_is_unavailable_without_internal() {
    ui_verification::run_cases(|test_cases| {
        test_cases.compile_fail("tests/ui/config_internal_reexport_unavailable.rs");
    })
    .expect("UI verification requires the provisioned exact MSRV/LLVM route; see resolver error");
}

#[test]
#[cfg(not(any(feature = "cli", feature = "internal")))]
fn stable_api_without_cli_compiles() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/stable_exec_context_no_cli.rs");
        test_cases.compile_fail("tests/ui/stable_embedder_cannot_use_engine_without_internal.rs");
    })
    .expect("UI verification requires the provisioned exact MSRV/LLVM route; see resolver error");
}

/// A valid fixture registered as compile-fail must fail during trybuild's drop.
#[test]
fn ui_registration_runs_and_propagates_destructor_failure() {
    use std::{
        cell::Cell,
        panic::{AssertUnwindSafe, catch_unwind},
    };

    temp_env::with_vars([] as [(&str, Option<&str>); 0], || {
        let parent_selector = std::env::var_os("RUSTUP_TOOLCHAIN");
        let registered = Cell::new(false);
        let execution = catch_unwind(AssertUnwindSafe(|| {
            ui_verification::run_cases(|cases| {
                assert_eq!(
                    std::env::var("RUSTUP_TOOLCHAIN").expect("callback selector exists"),
                    "1.88.0",
                    "the real registration callback must run inside the exact scope"
                );
                registered.set(true);
                cases.compile_fail("tests/ui/stable_exec_context_signatures.rs");
            })
        }));
        assert!(
            registered.get(),
            "the public boundary must invoke registration"
        );
        assert!(
            execution.is_err(),
            "a passing fixture registered as compile-fail must propagate trybuild's destructor failure"
        );
        assert_eq!(
            std::env::var_os("RUSTUP_TOOLCHAIN"),
            parent_selector,
            "the parent toolchain must be restored after the sentinel unwinds"
        );
    });
}
