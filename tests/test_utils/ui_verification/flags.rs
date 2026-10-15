//! Parses caller-provided rustflags for the isolated UI verification harness.
//!
//! The harness always receives its own diagnostic flags. Compatible LLVM
//! coverage inputs survive that boundary, while known development-build flags
//! and warning policy are removed before the encoded vector is assembled.

use super::VerificationError;

/// Delimiter used by Cargo for lossless argument encoding.
const UNIT_SEPARATOR: char = '\u{1f}';

/// String delimiter used when constructing the encoded rustflags value.
const UNIT_SEPARATOR_TEXT: &str = "\u{1f}";

/// Required flags that reconstruct trybuild's harness invocation.
const BASE_FLAGS: [&str; 6] = [
    "--cfg",
    "trybuild",
    "--verbose",
    "-A",
    "dead_code",
    "--diagnostic-width=140",
];

/// Build the encoded rustflags vector used by the UI verification child.
///
/// `CARGO_ENCODED_RUSTFLAGS` takes precedence over `RUSTFLAGS`, including when
/// its value is empty. The result always starts with the harness flags, then
/// appends compatible coverage, cfg, and path-remapping arguments in their
/// original order.
///
/// # Errors
///
/// Returns [`VerificationError::UnsupportedFlags`] if a caller flag is
/// malformed, dynamic, or outside the explicitly supported verification and
/// coverage set.
pub(super) fn verification_flags(
    encoded: Option<&str>,
    ordinary: Option<&str>,
) -> Result<String, VerificationError> {
    let arguments = input_arguments(encoded, ordinary)?;
    let retained = retained_arguments(&arguments)?;
    let mut result = BASE_FLAGS.map(str::to_owned).to_vec();
    result.extend(retained);
    Ok(result.join(UNIT_SEPARATOR_TEXT))
}

/// Select and split the authoritative environment representation.
fn input_arguments<'a>(
    encoded: Option<&'a str>,
    ordinary: Option<&'a str>,
) -> Result<Vec<&'a str>, VerificationError> {
    match encoded {
        Some("") => Ok(Vec::new()),
        Some(value) => {
            let arguments = value.split(UNIT_SEPARATOR).collect::<Vec<_>>();
            if arguments.iter().any(|argument| argument.is_empty()) {
                return Err(unsupported_flags("empty encoded flag"));
            }
            Ok(arguments)
        }
        None => {
            if ordinary.is_some_and(|value| value.contains(UNIT_SEPARATOR)) {
                return Err(unsupported_flags("unit separator in ordinary flags"));
            }
            Ok(ordinary.unwrap_or_default().split_whitespace().collect())
        }
    }
}

/// Keep only explicitly supported caller arguments after removing policy flags.
fn retained_arguments(arguments: &[&str]) -> Result<Vec<String>, VerificationError> {
    let mut retained = Vec::new();
    let mut remaining = arguments.iter().copied();

    while let Some(argument) = remaining.next() {
        validate_literal(argument)?;
        retain_argument(argument, &mut remaining, &mut retained)?;
    }

    Ok(retained)
}

/// Apply the allowlist to one argument and consume any required following value.
fn retain_argument<'a, I>(
    argument: &'a str,
    remaining: &mut I,
    retained: &mut Vec<String>,
) -> Result<(), VerificationError>
where
    I: Iterator<Item = &'a str>,
{
    match argument {
        "--cfg" => retain_cfg(next_value(remaining, "--cfg")?, retained),
        "--verbose" | "--diagnostic-width=140" => Ok(()),
        "-A" => discard_expected_value(remaining, "-A", "dead_code"),
        "-D" => discard_expected_value(remaining, "-D", "warnings"),
        "-C" => retain_codegen_flag(next_value(remaining, "-C")?, retained),
        "-Z" => discard_development_value(remaining),
        "--diagnostic-width" => discard_expected_value(remaining, "--diagnostic-width", "140"),
        "--remap-path-prefix" => {
            retain_remap(next_value(remaining, "--remap-path-prefix")?, retained)
        }
        _ => retain_compact_argument(argument, retained),
    }
}

/// Consume one exact policy value only for the parser's split option forms.
fn discard_expected_value<'a, I>(
    remaining: &mut I,
    option: &str,
    expected: &str,
) -> Result<(), VerificationError>
where
    I: Iterator<Item = &'a str>,
{
    let value = next_value(remaining, option)?;
    if value != expected {
        return Err(unsupported_flags(value));
    }
    Ok(())
}

/// Consume only the selected development flags from a split `-Z` option.
fn discard_development_value<'a, I>(remaining: &mut I) -> Result<(), VerificationError>
where
    I: Iterator<Item = &'a str>,
{
    let value = next_value(remaining, "-Z")?;
    if !is_development_flag("-Z", value) {
        return Err(unsupported_flags(value));
    }
    Ok(())
}

/// Apply the allowlist to compact `--option=value` and `-Cvalue` forms.
fn retain_compact_argument(
    argument: &str,
    retained: &mut Vec<String>,
) -> Result<(), VerificationError> {
    if let Some(value) = argument.strip_prefix("--cfg=") {
        return retain_cfg(value, retained);
    }
    if let Some(value) = argument.strip_prefix("-A") {
        return if value == "dead_code" {
            Ok(())
        } else {
            Err(unsupported_flags(argument))
        };
    }
    if let Some(value) = argument.strip_prefix("-D") {
        return if value == "warnings" {
            Ok(())
        } else {
            Err(unsupported_flags(argument))
        };
    }
    if let Some(value) = argument.strip_prefix("-C") {
        return retain_codegen_flag(value, retained);
    }
    if let Some(value) = argument.strip_prefix("-Z") {
        return if is_development_flag("-Z", value) {
            Ok(())
        } else {
            Err(unsupported_flags(argument))
        };
    }
    if let Some(value) = argument.strip_prefix("--diagnostic-width=") {
        return if value == "140" {
            Ok(())
        } else {
            Err(unsupported_flags(argument))
        };
    }
    if let Some(value) = argument.strip_prefix("--remap-path-prefix=") {
        return retain_remap(value, retained);
    }
    Err(unsupported_flags(argument))
}

/// Consume the value required by a two-token rustc option.
fn next_value<'a, I>(arguments: &mut I, option: &str) -> Result<&'a str, VerificationError>
where
    I: Iterator<Item = &'a str>,
{
    let value = arguments.next().ok_or_else(|| unsupported_flags(option))?;
    validate_literal(value)?;
    Ok(value)
}

/// Retain the supported cfg values while dropping harness cfg already present.
fn retain_cfg(value: &str, retained: &mut Vec<String>) -> Result<(), VerificationError> {
    match value {
        "trybuild" => Ok(()),
        "coverage" | "trybuild_no_target" => {
            retained.push(format!("--cfg={value}"));
            Ok(())
        }
        _ => Err(unsupported_flags(value)),
    }
}

/// Retain one explicitly compatible LLVM coverage codegen option.
fn retain_codegen_flag(value: &str, retained: &mut Vec<String>) -> Result<(), VerificationError> {
    match value {
        "instrument-coverage"
        | "instrument-coverage=yes"
        | "codegen-units=1"
        | "llvm-args=--instrprof-atomic-counter-update-all"
        | "link-arg=-Wl,--no-gc-sections" => {
            retained.push("-C".to_owned());
            retained.push(value.to_owned());
            Ok(())
        }
        "link-arg=-fuse-ld=mold" => Ok(()),
        _ => Err(unsupported_flags(value)),
    }
}

/// Retain a `from=to` remapping value, including an empty destination.
fn retain_remap(value: &str, retained: &mut Vec<String>) -> Result<(), VerificationError> {
    if !value.contains('=') {
        return Err(unsupported_flags(
            "--remap-path-prefix requires a from=to value",
        ));
    }
    retained.push("--remap-path-prefix".to_owned());
    retained.push(value.to_owned());
    Ok(())
}

/// Identify the exact development flags removed at the verification boundary.
fn is_development_flag(option: &str, value: &str) -> bool {
    matches!(
        (option, value),
        ("-Z", "threads=8" | "codegen-backend=cranelift")
    )
}

/// Reject shell-like text and control characters before interpreting a token.
fn validate_literal(value: &str) -> Result<(), VerificationError> {
    if value.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '\'' | '"' | '$' | '`' | ';' | '|' | '&' | '<' | '>'
            )
    }) {
        return Err(unsupported_flags(value));
    }
    Ok(())
}

/// Construct the typed error used for unsupported or malformed flag input.
fn unsupported_flags(value: &str) -> VerificationError {
    VerificationError::UnsupportedFlags(format!("unsupported or malformed rustflag: {value}"))
}

/// Parameterized contracts for the verification flag parser.
#[cfg(test)]
#[path = "flags_tests.rs"]
mod tests;
