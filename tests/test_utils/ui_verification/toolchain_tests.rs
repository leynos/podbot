//! Exact MSRV selector and real offline Rustup resolution contracts.

use rstest::rstest;

use super::{compatibility_toolchain, resolve_executables};

/// Partial manifest versions select their minimum patch, never a moving channel.
#[rstest]
#[case::partial("1.88", "1.88.0")]
#[case::explicit_patch("1.88.1", "1.88.1")]
fn compatibility_selector_preserves_the_declared_minimum(
    #[case] msrv: &str,
    #[case] expected: &str,
) {
    assert_eq!(
        compatibility_toolchain(msrv),
        expected,
        "selector must be exact"
    );
}

/// Real Rustup resolution must work with no usable distribution endpoint.
///
/// The endpoint and a decoy parent selector are set on the Rustup children
/// only; this process's environment is untouched.
///
/// Rustup installs a missing toolchain on first use, which the sibling
/// registration tests also trigger, and they no longer hold a process-wide lock
/// that ordered them first. A plain resolution therefore provisions the minimum
/// before the offline one, so the contract no longer depends on test order.
#[test]
fn provisioned_exact_minimum_resolves_without_distribution_access() {
    resolve_executables(env!("CARGO_PKG_RUST_VERSION"), &[])
        .expect("the exact MSRV must be installable or already provisioned");
    let executables = resolve_executables(
        env!("CARGO_PKG_RUST_VERSION"),
        &[
            ("RUSTUP_DIST_SERVER", "file:///podbot-ui-no-distribution"),
            ("RUSTUP_TOOLCHAIN", "nightly-parent"),
        ],
    )
    .expect("the provisioned exact MSRV must resolve offline through Rustup");
    assert_eq!(executables.toolchain, "1.88.0", "resolve the exact minimum");
}
