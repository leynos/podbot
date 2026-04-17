//! Compile-time tests asserting that CLI items are absent when the `cli`
//! feature is disabled.
//!
//! These tests use `trybuild` to verify that the Cargo feature gate in
//! `src/lib.rs` actually prevents `podbot::cli` from being accessible to
//! embedders that set `default-features = false`.

#[test]
fn cli_module_unavailable_without_feature() {
    // This assertion is only meaningful under `cargo test --no-default-features`.
    // With the default `cli` feature enabled, `podbot::cli` is expected to
    // exist and this compile-fail fixture would compile successfully.
    if cfg!(feature = "cli") {
        return;
    }

    let t = trybuild::TestCases::new();
    t.compile_fail("tests/cli_feature_gating/no_cli_feature.rs");
}
