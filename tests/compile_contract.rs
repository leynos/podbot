//! Compile-pass checks for the stable library boundary.

#[path = "test_utils/ui_verification/mod.rs"]
mod ui_verification;

#[test]
fn stable_exec_context_signatures_compile() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/stable_exec_context_signatures.rs");
        test_cases.pass("tests/ui/stable_run_request_signatures.rs");
        test_cases.pass("tests/ui/stable_embedder_uses_only_stable_modules.rs");
    })
    .expect(concat!(
        "UI verification requires the provisioned MSRV/LLVM route; run rustup toolchain install ",
        env!("CARGO_PKG_RUST_VERSION")
    ));
}

#[test]
fn stable_repository_clone_signatures_compile() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/stable_repository_clone_signatures.rs");
    })
    .expect(concat!(
        "UI verification requires the provisioned MSRV/LLVM route; run rustup toolchain install ",
        env!("CARGO_PKG_RUST_VERSION")
    ));
}

#[test]
#[cfg(feature = "experimental")]
fn experimental_run_agent_signature_compiles() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/experimental_run_agent_signatures.rs");
    })
    .expect(concat!(
        "UI verification requires the provisioned MSRV/LLVM route; run rustup toolchain install ",
        env!("CARGO_PKG_RUST_VERSION")
    ));
}

#[test]
#[cfg(feature = "internal")]
fn config_internal_reexport_is_available_with_internal() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/config_internal_reexport_available.rs");
    })
    .expect(concat!(
        "UI verification requires the provisioned MSRV/LLVM route; run rustup toolchain install ",
        env!("CARGO_PKG_RUST_VERSION")
    ));
}

#[test]
#[cfg(feature = "internal")]
fn exec_client_mock_contract_compiles() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/exec_client_mock_compiles.rs");
        test_cases.compile_fail("tests/ui/exec_client_mock_missing_step_result.rs");
        test_cases.compile_fail("tests/ui/exec_client_mock_malformed_invocation.rs");
    })
    .expect(concat!(
        "UI verification requires the provisioned MSRV/LLVM route; run rustup toolchain install ",
        env!("CARGO_PKG_RUST_VERSION")
    ));
}

#[test]
#[cfg(not(feature = "internal"))]
fn config_internal_reexport_is_unavailable_without_internal() {
    ui_verification::run_cases(|test_cases| {
        test_cases.compile_fail("tests/ui/config_internal_reexport_unavailable.rs");
    })
    .expect(concat!(
        "UI verification requires the provisioned MSRV/LLVM route; run rustup toolchain install ",
        env!("CARGO_PKG_RUST_VERSION")
    ));
}

#[test]
#[cfg(not(any(feature = "cli", feature = "internal")))]
fn stable_api_without_cli_compiles() {
    ui_verification::run_cases(|test_cases| {
        test_cases.pass("tests/ui/stable_exec_context_no_cli.rs");
        test_cases.compile_fail("tests/ui/stable_embedder_cannot_use_engine_without_internal.rs");
    })
    .expect(concat!(
        "UI verification requires the provisioned MSRV/LLVM route; run rustup toolchain install ",
        env!("CARGO_PKG_RUST_VERSION")
    ));
}
