"""Parse Cargo test options supported by the separated test runner."""

from __future__ import annotations

import dataclasses
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


@dataclasses.dataclass
class _CargoOptionState:
    """Mutable parse state private to one Cargo test argument list."""

    working_directory: pathlib.Path = dataclasses.field(
        default_factory=lambda: pathlib.Path.cwd().resolve()
    )
    common: list[str] = dataclasses.field(default_factory=list)
    selectors: list[tuple[str, str | None]] = dataclasses.field(default_factory=list)
    package_specs: list[str] = dataclasses.field(default_factory=list)
    excludes: list[str] = dataclasses.field(default_factory=list)
    positional: list[str] = dataclasses.field(default_factory=list)
    flags: set[str] = dataclasses.field(default_factory=set)
    manifest_path: pathlib.Path | None = None
    target_dir: pathlib.Path | None = None


def parse_cargo_test_options(
    arguments: list[str], *, cwd: pathlib.Path | None = None
) -> CargoTestOptions:
    """Parse shared build options, selectors, filters, and harness arguments.

    Examples
    --------
    >>> parsed = parse_cargo_test_options(["--test", "compile_contract", "api", "--", "--exact"])
    >>> parsed.test_filter, parsed.harness_args
    ('api', ('--exact',))
    """
    cargo_arguments, harness_arguments = _split_harness_arguments(arguments)
    state = _CargoOptionState(working_directory=(cwd or pathlib.Path.cwd()).resolve())
    index = 0
    while index < len(cargo_arguments):
        index = _consume_cargo_argument(cargo_arguments, index, state)
    _validate_options(state)
    return CargoTestOptions(
        common=tuple(state.common),
        selectors=tuple(state.selectors),
        package_specs=tuple(state.package_specs),
        excludes=tuple(state.excludes),
        workspace="--workspace" in state.flags,
        doc_only="--doc" in state.flags,
        no_run="--no-run" in state.flags,
        no_fail_fast="--no-fail-fast" in state.flags,
        test_filter=state.positional[0] if state.positional else None,
        harness_args=tuple(harness_arguments),
        manifest_path=state.manifest_path,
        target_dir=state.target_dir,
        all_targets="--all-targets" in state.flags,
    )


def _consume_cargo_argument(
    arguments: list[str], index: int, state: _CargoOptionState
) -> int:
    """Parse one Cargo argument and return the next argument index."""
    argument = arguments[index]
    option, separator, attached = argument.partition("=")
    attached_value = attached if separator else None
    if argument.startswith("-p") and not argument.startswith("--"):
        if len(argument) > 2 and not separator:
            option = "-p"
            attached_value = argument[2:]
    next_index = _consume_value_option(arguments, index, option, attached_value, state)
    if next_index is not None:
        return next_index
    next_index = _consume_boolean_option(option, separator, index, state)
    if next_index is not None:
        return next_index
    next_index = _consume_named_target_option(
        arguments, index, option, attached_value, state
    )
    if next_index is not None:
        return next_index
    if argument.startswith("-"):
        raise RunnerError(
            f"unsupported Cargo test option {argument!r}; "
            "the test runner must know how to map each option safely"
        )
    state.positional.append(argument)
    return index + 1


def _consume_value_option(
    arguments: list[str],
    index: int,
    option: str,
    attached: str | None,
    state: _CargoOptionState,
) -> int | None:
    """Consume one Cargo option that requires a value."""
    if option not in _VALUE_OPTIONS:
        return None
    value, next_index = _take_value(arguments, index, option, attached)
    if option in {"--manifest-path", "--target-dir"}:
        value = str((state.working_directory / value).resolve())
    _record_value_option(
        option,
        value,
        state.common,
        state.package_specs,
        state.excludes,
    )
    if option == "--manifest-path":
        state.manifest_path = pathlib.Path(value)
    if option == "--target-dir":
        state.target_dir = pathlib.Path(value)
    return next_index


def _consume_boolean_option(
    option: str, separator: str, index: int, state: _CargoOptionState
) -> int | None:
    """Consume a boolean Cargo option and record its selection effects."""
    if option not in _BOOLEAN_OPTIONS or separator:
        return None
    state.flags.add(option)
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
        state.common.append(option)
    match option:
        case "--all-targets":
            state.selectors.append(("all-targets", None))
        case "--lib":
            state.selectors.append(("lib", None))
        case "--bins" | "--examples" | "--tests" | "--benches":
            state.selectors.append((option[2:], None))
    return index + 1


def _consume_named_target_option(
    arguments: list[str],
    index: int,
    option: str,
    attached: str | None,
    state: _CargoOptionState,
) -> int | None:
    """Consume one named Cargo target selector such as `--test NAME`."""
    target_kind = _NAMED_TARGET_OPTIONS.get(option)
    if target_kind is None:
        return None
    value, next_index = _take_value(arguments, index, option, attached)
    state.selectors.append((target_kind, value))
    return next_index


def _validate_options(state: _CargoOptionState) -> None:
    """Reject test filters and target combinations Cargo cannot map safely."""
    if len(state.positional) > 1:
        raise RunnerError("Cargo test accepts at most one positional test filter")
    if "--doc" in state.flags and state.selectors:
        raise RunnerError("--doc cannot be combined with other target selectors")
    if state.excludes and "--workspace" not in state.flags:
        raise RunnerError("--exclude requires --workspace")


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
) -> None:
    """Keep shared Cargo options and record package selection separately."""
    if option in {"--package", "-p"}:
        package_specs.append(value)
        common.extend([option, value])
    elif option == "--exclude":
        excludes.append(value)
        common.extend([option, value])
    else:
        common.extend([option, value])
