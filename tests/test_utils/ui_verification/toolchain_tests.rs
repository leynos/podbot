//! Exact MSRV selector and real offline Rustup resolution contracts.

use rstest::rstest;

use super::{VerificationRoute, compatibility_toolchain, resolve_executables};

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
#[test]
fn provisioned_exact_minimum_resolves_without_distribution_access() {
    temp_env::with_vars(
        [
            (
                "RUSTUP_DIST_SERVER",
                Some("file:///podbot-ui-no-distribution"),
            ),
            ("RUSTUP_TOOLCHAIN", Some("nightly-parent")),
        ],
        || {
            let executables = resolve_executables(env!("CARGO_PKG_RUST_VERSION"))
                .expect("the provisioned exact MSRV must resolve offline through Rustup");
            assert_eq!(executables.toolchain, "1.88.0", "resolve the exact minimum");
            let route = VerificationRoute {
                encoded_flags: String::new(),
            };
            route.run(&executables, || {
                assert_eq!(
                    std::env::var("RUSTUP_TOOLCHAIN").expect("scoped selector exists"),
                    "1.88.0",
                    "nested Cargo must retain the exact resolved selector"
                );
            });
            assert_eq!(
                std::env::var("RUSTUP_TOOLCHAIN").expect("parent selector restored"),
                "nightly-parent",
                "resolution and scope must preserve the parent selector"
            );
        },
    );
}
