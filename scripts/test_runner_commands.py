"""Build Cargo phase arguments and explain phases stopped by fail-fast."""

from __future__ import annotations

import typing as typ

from test_runner_models import CargoTestPlan
from test_runner_options import attached_package_value


def without_message_format(arguments: tuple[str, ...]) -> tuple[str, ...]:
    """Remove an output mode so an artifact build can force JSON messages."""
    result: list[str] = []
    index = 0
    while index < len(arguments):
        argument = arguments[index]
        if argument == "--message-format":
            index += 2
        elif argument.startswith("--message-format="):
            index += 1
        else:
            result.append(argument)
            index += 1
    return tuple(result)


def without_package_selection(arguments: tuple[str, ...]) -> tuple[str, ...]:
    """Remove package filters already expanded from Cargo metadata."""
    result: list[str] = []
    index = 0
    while index < len(arguments):
        argument = arguments[index]
        skip_count = _package_selection_width(argument)
        if skip_count:
            index += skip_count
        else:
            result.append(argument)
            index += 1
    return tuple(result)


def print_skipped_phases(
    plan: CargoTestPlan,
    *,
    ordinary_offset: int = 0,
    include_doctests: bool = False,
    include_nested: bool = False,
) -> None:
    """Name later test phases that default fail-fast semantics skip."""
    remaining_packages = plan.ordinary_package_args[ordinary_offset:]
    if remaining_packages:
        package_names = ", ".join(name for name, _ in remaining_packages)
        print(
            f"== Skipping ordinary tests for remaining packages: {package_names} ==",
            flush=True,
        )
    if include_doctests and plan.run_doctests:
        print("== Skipping documentation tests after an earlier failure ==", flush=True)
    if include_nested and plan.nested_targets:
        print("== Skipping nested-Cargo tests after an earlier failure ==", flush=True)


def first_failure(statuses: typ.Iterable[int]) -> int:
    """Return the first non-zero phase status, or success when all pass."""
    return next((status for status in statuses if status != 0), 0)


def _package_selection_width(argument: str) -> int:
    """Return how many arguments one expanded package selector occupies."""
    if argument == "--workspace":
        return 1
    if attached_package_value(argument) is not None:
        return 1
    option, separator, _ = argument.partition("=")
    if option in {"--package", "-p", "--exclude"}:
        return 1 if separator else 2
    return 0
