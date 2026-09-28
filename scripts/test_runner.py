#!/usr/bin/env python3
"""Run Cargo tests while executing nested-Cargo contracts after Cargo exits."""

from __future__ import annotations

import argparse
import os
import pathlib
import shlex
import subprocess
import sys
import typing as typ

from test_runner_cargo import (
    create_test_runtime_environment,
    load_cargo_metadata,
    parse_cargo_json_message,
    select_test_executables,
)
from test_runner_models import CargoTestOptions, CargoTestPlan, RunnerError, Target
from test_runner_options import _attached_package_value, parse_cargo_test_options
from test_runner_plan import create_test_plan


def main(arguments: list[str] | None = None) -> int:
    """Run Cargo tests using the repository's nested-target registry.

    Examples
    --------
    >>> main(["--cargo", "false", "--", "--bad-option"]) != 0
    True
    """
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--cargo",
        default=os.environ.get("CARGO", "cargo"),
        help="Cargo command used for metadata and build phases",
    )
    parser.add_argument("cargo_arguments", nargs=argparse.REMAINDER)
    parsed = parser.parse_args(arguments)
    cargo_arguments = list(parsed.cargo_arguments)
    if cargo_arguments and cargo_arguments[0] == "--":
        cargo_arguments.pop(0)
    try:
        cargo_command = tuple(shlex.split(parsed.cargo))
        if not cargo_command:
            raise RunnerError("the Cargo command is empty")
        cwd = pathlib.Path.cwd().resolve()
        options = parse_cargo_test_options(cargo_arguments, cwd=cwd)
        metadata = load_cargo_metadata(cargo_command, options, cwd)
        plan = create_test_plan(metadata, options)
        return run_test_plan(cargo_command, plan, os.environ.copy())
    except (OSError, RunnerError) as exc:
        print(f"test runner: {exc}", file=sys.stderr)
        return 2


def run_test_plan(
    cargo_command: tuple[str, ...],
    plan: CargoTestPlan,
    environment: dict[str, str],
) -> int:
    """Run ordinary tests, doctests, and isolated nested-Cargo tests.

    Examples
    --------
    A compile-contract executable is started only after its `--no-run` Cargo
    process has returned successfully.
    """
    if plan.options.no_run:
        return _run_no_run(cargo_command, plan, environment)

    ordinary_results, should_stop = _run_ordinary_phase(
        cargo_command, plan, environment
    )
    if should_stop:
        _print_skipped_phases(
            plan,
            ordinary_offset=len(ordinary_results),
            include_doctests=True,
            include_nested=True,
        )
        return _first_failure(ordinary_results)

    doctest_results, should_stop = _run_doctest_phase(cargo_command, plan, environment)
    if should_stop:
        _print_skipped_phases(
            plan,
            ordinary_offset=len(plan.ordinary_package_args),
            include_nested=True,
        )
        return _first_failure((*ordinary_results, *doctest_results))

    nested_results, _ = _run_nested_phase(cargo_command, plan, environment)
    return _first_failure((*ordinary_results, *doctest_results, *nested_results))


def _run_ordinary_phase(
    cargo_command: tuple[str, ...], plan: CargoTestPlan, environment: dict[str, str]
) -> tuple[tuple[int, ...], bool]:
    """Run ordinary package tests and report whether fail-fast stopped work."""
    if not plan.ordinary_package_args:
        print("== No ordinary test targets selected; skipping phase ==", flush=True)
        return (), False
    statuses: list[int] = []
    for package_name, target_arguments in plan.ordinary_package_args:
        print(
            f"== Running ordinary Cargo test targets for {package_name} ==",
            flush=True,
        )
        status = _run_cargo_test(
            cargo_command,
            plan.options,
            target_arguments,
            plan.workspace_root,
            environment,
            package_name=package_name,
        )
        statuses.append(status)
        if status and not plan.options.no_fail_fast:
            return tuple(statuses), True
    return tuple(statuses), False


def _run_doctest_phase(
    cargo_command: tuple[str, ...], plan: CargoTestPlan, environment: dict[str, str]
) -> tuple[tuple[int, ...], bool]:
    """Run documentation tests and report whether fail-fast stopped work."""
    if not plan.run_doctests:
        print("== No documentation tests selected; skipping phase ==", flush=True)
        return (), False
    print("== Running Cargo documentation tests ==", flush=True)
    status = _run_cargo_test(
        cargo_command,
        plan.options,
        ("--doc",),
        plan.workspace_root,
        environment,
    )
    should_stop = bool(status and not plan.options.no_fail_fast)
    return (status,), should_stop


def _run_nested_phase(
    cargo_command: tuple[str, ...], plan: CargoTestPlan, environment: dict[str, str]
) -> tuple[tuple[int, ...], bool]:
    """Run nested-Cargo tests and report whether fail-fast stopped work."""
    if not plan.nested_targets:
        print("== No nested-Cargo targets selected; skipping phase ==", flush=True)
        return (), False
    statuses: list[int] = []
    for target in plan.nested_targets:
        status = _run_nested_target(cargo_command, plan, target, environment)
        statuses.append(status)
        if status and not plan.options.no_fail_fast:
            return tuple(statuses), True
    return tuple(statuses), False


def _run_no_run(
    cargo_command: tuple[str, ...], plan: CargoTestPlan, environment: dict[str, str]
) -> int:
    """Preserve Cargo's compile-only mode without launching any test process."""
    print("== Compiling selected tests without execution ==", flush=True)
    command = [*cargo_command, "test", *plan.options.common]
    command.extend(plan.selected_target_args)
    if plan.options.test_filter:
        command.append(plan.options.test_filter)
    command.append("--no-run")
    if plan.options.harness_args:
        command.extend(["--", *plan.options.harness_args])
    return _run_inherited(command, plan.workspace_root, environment)


def _run_cargo_test(
    cargo_command: tuple[str, ...],
    options: CargoTestOptions,
    target_arguments: tuple[str, ...],
    cwd: pathlib.Path,
    environment: dict[str, str],
    *,
    package_name: str | None = None,
) -> int:
    """Run one ordinary Cargo test phase with caller filters and flags."""
    common = (
        _without_package_selection(options.common)
        if package_name is not None
        else options.common
    )
    package_arguments = ["--package", package_name] if package_name else []
    command = [*cargo_command, "test", *common, *package_arguments, *target_arguments]
    if options.test_filter:
        command.append(options.test_filter)
    if options.harness_args:
        command.extend(["--", *options.harness_args])
    return _run_inherited(command, cwd, environment)


def _run_nested_target(
    cargo_command: tuple[str, ...],
    plan: CargoTestPlan,
    target: Target,
    environment: dict[str, str],
) -> int:
    """Build one registered test target, then invoke its current artifact."""
    key = (target.package_id, target.name)
    print(
        f"== Building nested-Cargo target {target.package_name}:{target.name} ==",
        flush=True,
    )
    command = [
        *cargo_command,
        "test",
        *_without_message_format(_without_package_selection(plan.options.common)),
        "--package",
        target.package_name,
        *target.cargo_selector(),
        "--no-run",
        "--message-format=json",
    ]
    messages: list[dict[str, typ.Any]] = []
    status = _run_json_build(command, plan.workspace_root, environment, messages)
    print(f"Cargo no-run build exited with status {status}.", flush=True)
    if status != 0:
        return status
    try:
        executable = select_test_executables(messages, (target,))[key]
    except RunnerError as exc:
        print(f"test runner: {exc}", file=sys.stderr)
        return 2
    package = next(
        (
            package
            for package in plan.selected_packages
            if package.get("id") == target.package_id
        ),
        None,
    )
    if package is None:
        print(
            "test runner: selected package "
            f"{target.package_name} is missing from the test plan",
            file=sys.stderr,
        )
        return 2
    test_environment = create_test_runtime_environment(
        environment,
        package,
        executable,
        messages,
        target_directory=plan.target_directory,
        cargo_command=cargo_command,
    )
    test_arguments = [plan.options.test_filter] if plan.options.test_filter else []
    test_arguments.extend(plan.options.harness_args)
    print(
        f"== Running {target.package_name}:{target.name} after Cargo exited ==",
        flush=True,
    )
    return _run_inherited(
        [str(executable), *test_arguments], target.manifest_dir, test_environment
    )


def _run_json_build(
    command: list[str],
    cwd: pathlib.Path,
    environment: dict[str, str],
    messages: list[dict[str, typ.Any]],
) -> int:
    """Stream a JSON-mode Cargo build and retain its current artifacts."""
    try:
        process_context = subprocess.Popen(
            command,
            cwd=cwd,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=None,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
    except OSError as exc:
        print(f"test runner: could not start Cargo: {exc}", file=sys.stderr)
        return 127
    with process_context as process:
        output = typ.cast(typ.TextIO, process.stdout)
        for line in output:
            sys.stdout.write(line)
            sys.stdout.flush()
            message = parse_cargo_json_message(line)
            if message is not None:
                messages.append(message)
        return process.wait()


def _run_inherited(
    command: list[str], cwd: pathlib.Path, environment: dict[str, str]
) -> int:
    """Run a command with inherited output and return its exit status."""
    try:
        return subprocess.run(command, cwd=cwd, env=environment, check=False).returncode
    except OSError as exc:
        print(f"test runner: could not start {command[0]}: {exc}", file=sys.stderr)
        return 127


def _without_message_format(arguments: tuple[str, ...]) -> tuple[str, ...]:
    """Remove an output mode so the artifact phase can force JSON messages."""
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


def _without_package_selection(arguments: tuple[str, ...]) -> tuple[str, ...]:
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


def _package_selection_width(argument: str) -> int:
    """Return how many arguments one expanded package selector occupies."""
    if argument == "--workspace":
        return 1
    if _attached_package_value(argument) is not None:
        return 1
    option, separator, _ = argument.partition("=")
    if option in {"--package", "-p", "--exclude"}:
        return 1 if separator else 2
    return 0


def _print_skipped_phases(
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


def _first_failure(statuses: typ.Iterable[int]) -> int:
    """Return the first non-zero phase status, or success when all pass."""
    return next((status for status in statuses if status != 0), 0)


if __name__ == "__main__":
    raise SystemExit(main())
