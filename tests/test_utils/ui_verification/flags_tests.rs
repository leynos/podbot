//! Parameterized contracts for the UI verification rustflags parser.

use proptest::prelude::*;
use rstest::rstest;

use super::super::VerificationError;
use super::{UNIT_SEPARATOR, verification_flags};

/// Exact harness vector required for every verification invocation.
const EXPECTED_BASE: &str =
    "--cfg\u{1f}trybuild\u{1f}--verbose\u{1f}-A\u{1f}dead_code\u{1f}--diagnostic-width=140";

/// Append a case-specific retained suffix to the independently stated base.
fn expected_flags(suffix: &str) -> String {
    if suffix.is_empty() {
        EXPECTED_BASE.to_owned()
    } else {
        format!("{EXPECTED_BASE}{UNIT_SEPARATOR}{suffix}")
    }
}

/// Verify that each supported coverage or path input survives encoding.
#[rstest]
#[case::instrument_coverage(
    Some("-C\u{1f}instrument-coverage"),
    None,
    "-C\u{1f}instrument-coverage"
)]
#[case::instrument_coverage_yes(
    None,
    Some("-C instrument-coverage=yes"),
    "-C\u{1f}instrument-coverage=yes"
)]
#[case::single_codegen_unit(Some("-Ccodegen-units=1"), None, "-C\u{1f}codegen-units=1")]
#[case::atomic_coverage_counters(
    Some("-C\u{1f}llvm-args=--instrprof-atomic-counter-update-all"),
    None,
    "-C\u{1f}llvm-args=--instrprof-atomic-counter-update-all"
)]
#[case::windows_coverage_linker(
    None,
    Some("-C link-arg=-Wl,--no-gc-sections"),
    "-C\u{1f}link-arg=-Wl,--no-gc-sections"
)]
#[case::legacy_llvm_coverage_route(
    Some(concat!(
        "-C\u{1f}instrument-coverage\u{1f}",
        "-C\u{1f}codegen-units=1\u{1f}",
        "-C\u{1f}llvm-args=--instrprof-atomic-counter-update-all\u{1f}",
        "--cfg=coverage\u{1f}--cfg=trybuild_no_target\u{1f}",
        "--remap-path-prefix\u{1f}src=/workspace\u{1f}",
        "-C\u{1f}link-arg=-Wl,--no-gc-sections",
    )),
    None,
    concat!(
        "-C\u{1f}instrument-coverage\u{1f}",
        "-C\u{1f}codegen-units=1\u{1f}",
        "-C\u{1f}llvm-args=--instrprof-atomic-counter-update-all\u{1f}",
        "--cfg=coverage\u{1f}--cfg=trybuild_no_target\u{1f}",
        "--remap-path-prefix\u{1f}src=/workspace\u{1f}",
        "-C\u{1f}link-arg=-Wl,--no-gc-sections",
    ),
)]
#[case::coverage_cfg(Some("--cfg=coverage"), None, "--cfg=coverage")]
#[case::trybuild_no_target_cfg(
    Some("--cfg\u{1f}trybuild_no_target"),
    None,
    "--cfg=trybuild_no_target"
)]
#[case::remap_equals(
    Some("--remap-path-prefix=src=/workspace"),
    None,
    "--remap-path-prefix\u{1f}src=/workspace"
)]
#[case::remap_separate(
    None,
    Some("--remap-path-prefix src=/workspace"),
    "--remap-path-prefix\u{1f}src=/workspace"
)]
#[case::encoded_remap_path_with_spaces(
    Some("--remap-path-prefix\u{1f}src=/workspace with spaces"),
    None,
    "--remap-path-prefix\u{1f}src=/workspace with spaces"
)]
#[case::empty_remap_destination(
    Some("--remap-path-prefix\u{1f}/workspace/="),
    None,
    "--remap-path-prefix\u{1f}/workspace/="
)]
fn compatible_inputs_are_retained(
    #[case] encoded: Option<&str>,
    #[case] ordinary: Option<&str>,
    #[case] expected_suffix: &str,
) {
    let actual = verification_flags(encoded, ordinary).expect("supported flags parse");
    assert_eq!(
        actual,
        expected_flags(expected_suffix),
        "compatible coverage and path flags must survive verification routing",
    );
}

/// Verify that encoded flags override ordinary flags and exact B policy flags drop.
#[rstest]
#[case::no_flags(None, None, "")]
#[case::encoded_precedes_ordinary(
    Some("-C\u{1f}instrument-coverage"),
    Some("-Zthreads=8 -D warnings"),
    "-C\u{1f}instrument-coverage"
)]
#[case::empty_encoded_precedes_ordinary(Some(""), Some("--cfg=coverage"), "")]
#[case::known_development_flags_are_removed(
    Some(concat!(
        "-Zthreads=8\u{1f}-Zcodegen-backend=cranelift\u{1f}",
        "-Clink-arg=-fuse-ld=mold\u{1f}-D\u{1f}warnings",
    )),
    None,
    "",
)]
#[case::known_split_development_flags_are_removed(
    Some(concat!(
        "-Z\u{1f}threads=8\u{1f}-Z\u{1f}codegen-backend=cranelift\u{1f}",
        "-C\u{1f}link-arg=-fuse-ld=mold",
    )),
    None,
    "",
)]
#[case::duplicate_harness_flags_are_removed(
    Some(concat!(
        "--cfg=trybuild\u{1f}--verbose\u{1f}-Adead_code\u{1f}",
        "--diagnostic-width\u{1f}140",
    )),
    None,
    "",
)]
fn encoded_source_and_harness_policy_are_applied(
    #[case] encoded: Option<&str>,
    #[case] ordinary: Option<&str>,
    #[case] expected_suffix: &str,
) {
    assert_eq!(
        verification_flags(encoded, ordinary).expect("supported flags parse"),
        expected_flags(expected_suffix),
        "verification routing must prioritize encoded flags and remove only known policy flags",
    );
}

/// Verify that nightly, unknown, dynamic, and incomplete options fail closed.
#[rstest]
#[case::nightly_coverage_cfg(Some("--cfg=coverage_nightly"), None)]
#[case::unstable_coverage_options(Some("-Zcoverage-options=branch"), None)]
#[case::unknown_backend(Some("-Zcodegen-backend=llvm"), None)]
#[case::unknown_linker(Some("-Clink-arg=-fuse-ld=lld"), None)]
#[case::unknown_coverage_mode(Some("-Cinstrument-coverage=no"), None)]
#[case::quoted_value(Some("--remap-path-prefix=\"src=/workspace\""), None)]
#[case::dynamic_value(None, Some("--remap-path-prefix=src=$WORKSPACE"))]
#[case::missing_cfg_value(Some("--cfg"), None)]
#[case::missing_codegen_value(Some("-C"), None)]
#[case::missing_remap_value(Some("--remap-path-prefix"), None)]
#[case::remap_value_without_separator(Some("--remap-path-prefix=src/workspace"), None)]
#[case::wrong_diagnostic_width(Some("--diagnostic-width=120"), None)]
fn unsupported_inputs_are_rejected(#[case] encoded: Option<&str>, #[case] ordinary: Option<&str>) {
    assert!(
        matches!(
            verification_flags(encoded, ordinary),
            Err(VerificationError::UnsupportedFlags(_))
        ),
        "unsupported flags must fail closed"
    );
}

proptest! {
    /// Preserve arbitrary safe remapping prefixes, including embedded spaces.
    #[test]
    fn encoded_remap_values_are_lossless(source in "[A-Za-z0-9 _./:-]{1,40}") {
        let encoded = format!("--remap-path-prefix{UNIT_SEPARATOR}{source}=/workspace");
        let suffix = format!("--remap-path-prefix{UNIT_SEPARATOR}{source}=/workspace");
        match verification_flags(Some(&encoded), None) {
            Ok(actual) => prop_assert_eq!(
                actual,
                expected_flags(&suffix),
                "encoded remapping values must be preserved byte-for-byte",
            ),
            Err(error) => prop_assert!(
                false,
                "safe encoded remapping value was rejected: {error}",
            ),
        }
    }

    /// Reject every unrecognized UI flag in the generated lowercase corpus.
    #[test]
    fn unknown_ui_flags_are_unsupported(suffix in "[a-z]{1,20}") {
        let unknown = format!("--ui-unknown-{suffix}");
        prop_assert!(
            matches!(
                verification_flags(Some(&unknown), None),
                Err(VerificationError::UnsupportedFlags(_)),
            ),
            "unknown UI flag must produce UnsupportedFlags: {unknown}",
        );
    }
}

/// Preserve option spelling, value diagnostics, and first-error precedence.
#[rstest]
#[case::compact_allow_value("-Aunused", "-Aunused")]
#[case::compact_deny_value("-Ddead_code", "-Ddead_code")]
#[case::compact_unstable_value("-Zcodegen-backend=llvm", "-Zcodegen-backend=llvm")]
#[case::compact_width_value("--diagnostic-width=120", "--diagnostic-width=120")]
#[case::cfg_prefix_must_be_exact("--cfg-extra=coverage", "--cfg-extra=coverage")]
#[case::remap_prefix_must_be_exact(
    "--remap-path-prefix-extra=src=/workspace",
    "--remap-path-prefix-extra=src=/workspace"
)]
#[case::policy_value_must_be_exact("-Adead_code_extra", "-Adead_code_extra")]
#[case::compact_cfg_value("--cfg=unknown", "unknown")]
#[case::compact_codegen_value("-Ccodegen-units=2", "codegen-units=2")]
#[case::empty_compact_cfg_value("--cfg=", "")]
#[case::dynamic_compact_value("--cfg=$VAR", "--cfg=$VAR")]
#[case::dynamic_split_value("--cfg\u{1f}$VAR", "$VAR")]
#[case::early_compact_error_before_later_dynamic_text(
    "--cfg=coverage\u{1f}-Aunused\u{1f}--cfg=$BAD",
    "-Aunused"
)]
#[case::early_split_error_before_later_dynamic_text(
    "--cfg=coverage\u{1f}-A\u{1f}unused\u{1f}--cfg=$BAD",
    "unused"
)]
#[case::encoded_shape_error_precedes_token_policy("-Aunused\u{1f}", "empty encoded flag")]
#[case::dynamic_split_remap_value("--remap-path-prefix\u{1f}src=$WORKSPACE", "src=$WORKSPACE")]
#[case::missing_unstable("-Z", "-Z")]
#[case::missing_allow("-A", "-A")]
#[case::wrong_allow("-A\u{1f}unused", "unused")]
#[case::missing_deny("-D", "-D")]
#[case::wrong_deny("-D\u{1f}dead_code", "dead_code")]
#[case::wrong_unstable("-Z\u{1f}codegen-backend=llvm", "codegen-backend=llvm")]
#[case::missing_width("--diagnostic-width", "--diagnostic-width")]
#[case::wrong_width("--diagnostic-width\u{1f}120", "120")]
#[case::first_error_wins("-A\u{1f}unused\u{1f}--unknown", "unused")]
fn rejected_values_preserve_the_first_error(#[case] encoded: &str, #[case] rejected: &str) {
    let error = verification_flags(Some(encoded), None).expect_err("invalid option must fail");
    assert!(
        matches!(error, VerificationError::UnsupportedFlags(detail)
        if detail == format!("unsupported or malformed rustflag: {rejected}")),
        "the original offending option or value must remain the diagnostic"
    );
}
