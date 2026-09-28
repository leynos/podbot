"""Parse Cargo test options supported by the separated test runner."""

from __future__ import annotations

import pathlib

from test_runner_models import CargoTestOptions, RunnerError


_VALUE_OPTIONS = {
    "--features",
    "-F",
    "--target",
    "--target-dir",
    "--manifest-path",
    "--profile",
    "--jobs",
    "-j",
    "--package",
    "-p",
    "--exclude",
    "--color",
    "--message-format",
}
_BOOLEAN_OPTIONS = {
    "--all-features",
    "--no-default-features",
    "--release",
    "--workspace",
    "--all-targets",
    "--doc",
    "--lib",
    "--bins",
    "--examples",
    "--tests",
    "--benches",
    "--no-run",
    "--no-fail-fast",
    "--offline",
    "--locked",
    "--frozen",
    "--quiet",
    "-q",
    "--verbose",
    "-v",
}
_NAMED_TARGET_OPTIONS = {
    "--bin": "bin",
    "--test": "test",
    "--example": "example",
    "--bench": "bench",
}


def parse_cargo_test_options(arguments: list[str]) -> CargoTestOptions:
    """Parse shared build options, selectors, filters, and harness arguments.

    Examples
    --------
    >>> parsed = parse_cargo_test_options(["--test", "compile_contract", "api", "--", "--exact"])
    >>> parsed.test_filter, parsed.harness_args
    ('api', ('--exact',))
    """
    cargo_arguments, harness_arguments = _split_harness_arguments(arguments)
    common: list[str] = []
    selectors: list[tuple[str, str | None]] = []
    package_specs: list[str] = []
    excludes: list[str] = []
    positional: list[str] = []
    manifest_path: pathlib.Path | None = None
    target_dir: pathlib.Path | None = None
    flags: set[str] = set()
    index = 0

    while index < len(cargo_arguments):
        argument = cargo_arguments[index]
        option, separator, attached = argument.partition("=")
        if option in _VALUE_OPTIONS:
            value, index = _take_value(
                cargo_arguments, index, option, attached if separator else None
            )
            _record_value_option(
                option,
                value,
                common,
                package_specs,
                excludes,
                selectors,
            )
            if option == "--manifest-path":
                manifest_path = pathlib.Path(value)
            if option == "--target-dir":
                target_dir = pathlib.Path(value)
            continue
        if option in _BOOLEAN_OPTIONS and not separator:
            flags.add(option)
            if option not in {
                "--all-targets",
                "--no-run",
                "--doc",
                "--lib",
                "--bins",
                "--examples",
                "--tests",
                "--benches",
            }:
                common.append(option)
            if option == "--all-targets":
                selectors.append(("all-targets", None))
            elif option == "--lib":
                selectors.append(("lib", None))
            elif option in {"--bins", "--examples", "--tests", "--benches"}:
                selectors.append((option[2:], None))
            index += 1
            continue
        if option in _NAMED_TARGET_OPTIONS:
            value, index = _take_value(
                cargo_arguments, index, option, attached if separator else None
            )
            selectors.append((_NAMED_TARGET_OPTIONS[option], value))
            continue
        if argument.startswith("-"):
            raise RunnerError(
                f"unsupported Cargo test option {argument!r}; "
                "the test runner must know how to map each option safely"
            )
        positional.append(argument)
        index += 1

    if len(positional) > 1:
        raise RunnerError("Cargo test accepts at most one positional test filter")
    if "--doc" in flags and selectors:
        raise RunnerError("--doc cannot be combined with other target selectors")
    if excludes and "--workspace" not in flags:
        raise RunnerError("--exclude requires --workspace")
    return CargoTestOptions(
        common=tuple(common),
        selectors=tuple(selectors),
        package_specs=tuple(package_specs),
        excludes=tuple(excludes),
        workspace="--workspace" in flags,
        doc_only="--doc" in flags,
        no_run="--no-run" in flags,
        no_fail_fast="--no-fail-fast" in flags,
        test_filter=positional[0] if positional else None,
        harness_args=tuple(harness_arguments),
        manifest_path=manifest_path,
        target_dir=target_dir,
        all_targets="--all-targets" in flags,
    )


def _split_harness_arguments(arguments: list[str]) -> tuple[list[str], list[str]]:
    """Separate Cargo options from libtest arguments."""
    if "--" not in arguments:
        return arguments, []
    marker = arguments.index("--")
    return arguments[:marker], arguments[marker + 1 :]


def _take_value(
    arguments: list[str], index: int, option: str, attached: str | None
) -> tuple[str, int]:
    """Read one required option value, accepting `--option=value`."""
    if attached is not None:
        if not attached:
            raise RunnerError(f"{option} requires a value")
        return attached, index + 1
    if index + 1 >= len(arguments) or arguments[index + 1].startswith("-"):
        raise RunnerError(f"{option} requires a value")
    return arguments[index + 1], index + 2


def _record_value_option(
    option: str,
    value: str,
    common: list[str],
    package_specs: list[str],
    excludes: list[str],
    selectors: list[tuple[str, str | None]],
) -> None:
    """Keep shared Cargo options and extract package and target selectors."""
    if option in {"--package", "-p"}:
        package_specs.append(value)
        common.extend([option, value])
    elif option == "--exclude":
        excludes.append(value)
        common.extend([option, value])
    elif option in _NAMED_TARGET_OPTIONS:
        selectors.append((_NAMED_TARGET_OPTIONS[option], value))
    else:
        common.extend([option, value])
