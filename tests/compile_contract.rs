//! Compile-pass checks for the stable library boundary.

#[test]
fn stable_exec_context_signatures_compile() {
    let test_cases = trybuild::TestCases::new();
    test_cases.pass("tests/ui/stable_exec_context_signatures.rs");
    test_cases.pass("tests/ui/stable_embedder_uses_only_stable_modules.rs");
}

#[test]
#[cfg(not(any(feature = "cli", feature = "internal")))]
fn stable_api_without_cli_compiles() {
    let test_cases = trybuild::TestCases::new();
    test_cases.pass("tests/ui/stable_exec_context_no_cli.rs");
    test_cases.compile_fail("tests/ui/stable_embedder_cannot_use_engine_without_internal.rs");
}
