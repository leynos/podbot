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

/// One caller token already checked for shell syntax and control characters.
///
/// Only `parse` admits caller input; a compact option's suffix inherits the
/// same literal guarantee because it is a substring of that checked token.
#[derive(Clone, Copy)]
struct FlagToken<'a>(&'a str);

impl<'a> FlagToken<'a> {
    /// Reject dynamic text before an option or its consumed value is interpreted.
    fn parse(value: &'a str) -> Result<Self, VerificationError> {
        if value.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '\'' | '"' | '$' | '`' | ';' | '|' | '&' | '<' | '>'
                )
        }) {
            return Err(unsupported_flags(value));
        }
        Ok(Self(value))
    }

    /// Separate an exact supported compact prefix from its literal value.
    fn compact_option(self) -> Option<(CompactOption, Self)> {
        COMPACT_OPTIONS.iter().find_map(|(prefix, option)| {
            self.0
                .strip_prefix(*prefix)
                .map(|value| (*option, Self(value)))
        })
    }
}

/// Finite option grammar for compact rustflags; values are checked separately.
#[derive(Clone, Copy)]
enum CompactOption {
    /// Conditional compilation option.
    Cfg,
    /// Allowed-lint policy.
    Allow,
    /// Denied-lint policy.
    Deny,
    /// LLVM code generation option.
    Codegen,
    /// Nightly development option.
    Unstable,
    /// Diagnostic display width.
    DiagnosticWidth,
    /// Source path remapping.
    Remap,
}

/// Compact spelling is explicit: long options require their `=` separator.
const COMPACT_OPTIONS: [(&str, CompactOption); 7] = [
    ("--cfg=", CompactOption::Cfg),
    ("-A", CompactOption::Allow),
    ("-D", CompactOption::Deny),
    ("-C", CompactOption::Codegen),
    ("-Z", CompactOption::Unstable),
    ("--diagnostic-width=", CompactOption::DiagnosticWidth),
    ("--remap-path-prefix=", CompactOption::Remap),
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
        let token = FlagToken::parse(argument)?;
        retain_argument(token, &mut remaining, &mut retained)?;
    }

    Ok(retained)
}

/// Apply the allowlist to one argument and consume any required following value.
fn retain_argument<'a, I>(
    argument: FlagToken<'a>,
    remaining: &mut I,
    retained: &mut Vec<String>,
) -> Result<(), VerificationError>
where
    I: Iterator<Item = &'a str>,
{
    match argument.0 {
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
    if value.0 != expected {
        return Err(unsupported_flags(value.0));
    }
    Ok(())
}

/// Consume only the selected development flags from a split `-Z` option.
fn discard_development_value<'a, I>(remaining: &mut I) -> Result<(), VerificationError>
where
    I: Iterator<Item = &'a str>,
{
    let value = next_value(remaining, "-Z")?;
    if !is_development_flag(value) {
        return Err(unsupported_flags(value.0));
    }
    Ok(())
}

/// Apply the allowlist to compact `--option=value` and `-Cvalue` forms.
fn retain_compact_argument(
    argument: FlagToken<'_>,
    retained: &mut Vec<String>,
) -> Result<(), VerificationError> {
    let Some((option, value)) = argument.compact_option() else {
        return Err(unsupported_flags(argument.0));
    };
    match (option, value.0) {
        (CompactOption::Cfg, _) => retain_cfg(value, retained),
        (CompactOption::Codegen, _) => retain_codegen_flag(value, retained),
        (CompactOption::Remap, _) => retain_remap(value, retained),
        (CompactOption::Allow, "dead_code")
        | (CompactOption::Deny, "warnings")
        | (CompactOption::DiagnosticWidth, "140") => Ok(()),
        (CompactOption::Unstable, _) if is_development_flag(value) => Ok(()),
        _ => Err(unsupported_flags(argument.0)),
    }
}

/// Consume the value required by a two-token rustc option.
fn next_value<'a, I>(arguments: &mut I, option: &str) -> Result<FlagToken<'a>, VerificationError>
where
    I: Iterator<Item = &'a str>,
{
    let value = arguments.next().ok_or_else(|| unsupported_flags(option))?;
    FlagToken::parse(value)
}

/// Retain the supported cfg values while dropping harness cfg already present.
fn retain_cfg(value: FlagToken<'_>, retained: &mut Vec<String>) -> Result<(), VerificationError> {
    match value.0 {
        "trybuild" => Ok(()),
        "coverage" | "trybuild_no_target" => {
            retained.push(format!("--cfg={}", value.0));
            Ok(())
        }
        _ => Err(unsupported_flags(value.0)),
    }
}

/// Retain one explicitly compatible LLVM coverage codegen option.
fn retain_codegen_flag(
    value: FlagToken<'_>,
    retained: &mut Vec<String>,
) -> Result<(), VerificationError> {
    match value.0 {
        "instrument-coverage"
        | "instrument-coverage=yes"
        | "codegen-units=1"
        | "llvm-args=--instrprof-atomic-counter-update-all"
        | "link-arg=-Wl,--no-gc-sections" => {
            retained.push("-C".to_owned());
            retained.push(value.0.to_owned());
            Ok(())
        }
        "link-arg=-fuse-ld=mold" => Ok(()),
        _ => Err(unsupported_flags(value.0)),
    }
}

/// Retain a `from=to` remapping value, including an empty destination.
fn retain_remap(value: FlagToken<'_>, retained: &mut Vec<String>) -> Result<(), VerificationError> {
    if !value.0.contains('=') {
        return Err(unsupported_flags(
            "--remap-path-prefix requires a from=to value",
        ));
    }
    retained.push("--remap-path-prefix".to_owned());
    retained.push(value.0.to_owned());
    Ok(())
}

/// Identify the exact development flags removed at the verification boundary.
fn is_development_flag(value: FlagToken<'_>) -> bool {
    matches!(value.0, "threads=8" | "codegen-backend=cranelift")
}

/// Construct the typed error used for unsupported or malformed flag input.
fn unsupported_flags(value: &str) -> VerificationError {
    VerificationError::UnsupportedFlags(format!("unsupported or malformed rustflag: {value}"))
}

/// Parameterized contracts for the verification flag parser.
#[cfg(test)]
#[path = "flags_tests.rs"]
mod tests;
