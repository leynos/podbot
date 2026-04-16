//! Compile-pass checks for the stable library boundary.

#[test]
fn stable_exec_context_signatures_compile() {
    let test_cases = trybuild::TestCases::new();
    test_cases.pass("tests/ui/stable_exec_context_signatures.rs");
}
