#!/usr/bin/env python3
"""Run Cargo tests while executing nested-Cargo contracts after Cargo exits.

This runner separates compilation from execution for targets that invoke
nested Cargo, so the parent Cargo process has exited before trybuild starts.
All other selected Cargo tests and the default doctest phase remain available.

Usage
-----
Pass Cargo test arguments after the runner's ``--`` separator, for example:

    python scripts/test_runner.py -- --all-targets --all-features
"""

from __future__ import annotations

import argparse
import os
import pathlib
import shlex
import sys
import typing as typ

from test_runner_cargo import (
    RunnerCommandFailure,
    create_test_runtime_environment,
    load_cargo_metadata,
    parse_cargo_json_message,
    select_test_executables,
)
from test_runner_models import CargoTestOptions, CargoTestPlan, RunnerError, Target
from test_runner_commands import (
    first_failure,
    print_skipped_phases,
    without_message_format,
    without_package_selection,
)
from test_runner_options import parse_cargo_test_options
from test_runner_plan import create_test_plan
from test_runner_supervisor import ProcessSupervisor


def main(arguments: list[str] | None = None, *, enable_subreaper: bool = False) -> int:
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
    parser.add_argument(
        "--timeout",
        type=_positive_float,
        default=os.environ.get("PODBOT_TEST_TIMEOUT", "1800"),
        help="maximum total run time in seconds (default: 1800)",
    )
    parser.add_argument(
        "--watch-interval",
        type=_positive_float,
        default=30.0,
        help="seconds between process and lock diagnostic snapshots",
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
        environment = os.environ.copy()
        with ProcessSupervisor(
            parsed.timeout,
            parsed.watch_interval,
            enable_subreaper=enable_subreaper,
        ) as supervisor:
            metadata = load_cargo_metadata(
                cargo_command, options, cwd, environment, supervisor
            )
            plan = create_test_plan(metadata, options)
            environment["CARGO_TARGET_DIR"] = str(plan.target_directory)
            return run_test_plan(cargo_command, plan, environment, supervisor)
    except RunnerCommandFailure as exc:
        return exc.status
    except (OSError, RunnerError) as exc:
        print(f"test runner: {exc}", file=sys.stderr)
        return 2


def run_test_plan(
    cargo_command: tuple[str, ...],
    plan: CargoTestPlan,
    environment: dict[str, str],
    supervisor: ProcessSupervisor,
) -> int:
    """Run ordinary tests, doctests, and isolated nested-Cargo tests.

    Examples
    --------
    A compile-contract executable is started only after its `--no-run` Cargo
    process has returned successfully.
    """
    if plan.options.no_run:
        return _run_no_run(cargo_command, plan, environment, supervisor)

    ordinary_results, should_stop = _run_ordinary_phase(
        cargo_command, plan, environment, supervisor
    )
    if supervisor.terminal_status is not None:
        return supervisor.terminal_status
    if should_stop:
        print_skipped_phases(
            plan,
            ordinary_offset=len(ordinary_results),
            include_doctests=True,
            include_nested=True,
        )
        return first_failure(ordinary_results)

    doctest_results, should_stop = _run_doctest_phase(
        cargo_command, plan, environment, supervisor
    )
    if supervisor.terminal_status is not None:
        return supervisor.terminal_status
    if should_stop:
        print_skipped_phases(
            plan,
            ordinary_offset=len(plan.ordinary_package_args),
            include_nested=True,
        )
        return first_failure((*ordinary_results, *doctest_results))

    nested_results, _ = _run_nested_phase(cargo_command, plan, environment, supervisor)
    if supervisor.terminal_status is not None:
        return supervisor.terminal_status
    return first_failure((*ordinary_results, *doctest_results, *nested_results))


def _run_ordinary_phase(
    cargo_command: tuple[str, ...],
    plan: CargoTestPlan,
    environment: dict[str, str],
    supervisor: ProcessSupervisor,
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
            supervisor,
            package_name=package_name,
        )
        statuses.append(status)
        if supervisor.terminal_status is not None:
            return tuple(statuses), True
        if status and not plan.options.no_fail_fast:
            return tuple(statuses), True
    return tuple(statuses), False


def _run_doctest_phase(
    cargo_command: tuple[str, ...],
    plan: CargoTestPlan,
    environment: dict[str, str],
    supervisor: ProcessSupervisor,
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
        supervisor,
    )
    should_stop = bool(status and not plan.options.no_fail_fast)
    return (status,), should_stop


def _run_nested_phase(
    cargo_command: tuple[str, ...],
    plan: CargoTestPlan,
    environment: dict[str, str],
    supervisor: ProcessSupervisor,
) -> tuple[tuple[int, ...], bool]:
    """Run nested-Cargo tests and report whether fail-fast stopped work."""
    if not plan.nested_targets:
        print("== No nested-Cargo targets selected; skipping phase ==", flush=True)
        return (), False
    statuses: list[int] = []
    for target in plan.nested_targets:
        status = _run_nested_target(
            cargo_command, plan, target, environment, supervisor
        )
        statuses.append(status)
        if supervisor.terminal_status is not None:
            return tuple(statuses), True
        if status and not plan.options.no_fail_fast:
            return tuple(statuses), True
    return tuple(statuses), False


def _run_no_run(
    cargo_command: tuple[str, ...],
    plan: CargoTestPlan,
    environment: dict[str, str],
    supervisor: ProcessSupervisor,
) -> int:
    """Preserve Cargo's compile-only mode without launching test harnesses.

    Selected nested-Cargo targets are included in the build, but their test
    executables remain unlaunched because `--no-run` applies to every target.
    """
    print("== Compiling selected tests without execution ==", flush=True)
    command = [*cargo_command, "test", *plan.options.common]
    command.extend(plan.selected_target_args)
    if plan.options.test_filter:
        command.append(plan.options.test_filter)
    command.append("--no-run")
    if plan.options.harness_args:
        command.extend(["--", *plan.options.harness_args])
    return supervisor.run_inherited(
        command, plan.workspace_root, environment, purpose="Cargo test --no-run"
    )


def _run_cargo_test(
    cargo_command: tuple[str, ...],
    options: CargoTestOptions,
    target_arguments: tuple[str, ...],
    cwd: pathlib.Path,
    environment: dict[str, str],
    supervisor: ProcessSupervisor,
    *,
    package_name: str | None = None,
) -> int:
    """Run one ordinary Cargo test phase with caller filters and flags."""
    common = (
        without_package_selection(options.common)
        if package_name is not None
        else options.common
    )
    package_arguments = ["--package", package_name] if package_name else []
    command = [*cargo_command, "test", *common, *package_arguments, *target_arguments]
    if options.test_filter:
        command.append(options.test_filter)
    if options.harness_args:
        command.extend(["--", *options.harness_args])
    return supervisor.run_inherited(
        command, cwd, environment, purpose="ordinary Cargo tests"
    )


def _run_nested_target(
    cargo_command: tuple[str, ...],
    plan: CargoTestPlan,
    target: Target,
    environment: dict[str, str],
    supervisor: ProcessSupervisor,
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
        *without_message_format(without_package_selection(plan.options.common)),
        "--package",
        target.package_name,
        *target.cargo_selector(),
        "--no-run",
        "--message-format=json",
    ]
    messages: list[dict[str, typ.Any]] = []
    status = _run_json_build(
        command, plan.workspace_root, environment, messages, supervisor
    )
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
    return supervisor.run_inherited(
        [str(executable), *test_arguments],
        target.manifest_dir,
        test_environment,
        purpose=f"{target.package_name}:{target.name} test harness",
    )


def _run_json_build(
    command: list[str],
    cwd: pathlib.Path,
    environment: dict[str, str],
    messages: list[dict[str, typ.Any]],
    supervisor: ProcessSupervisor,
) -> int:
    """Stream a JSON-mode Cargo build and retain its current artifacts."""

    def emit_line(line: str) -> None:
        sys.stdout.write(line)
        sys.stdout.flush()
        message = parse_cargo_json_message(line)
        if message is not None:
            messages.append(message)

    return supervisor.run_lines(
        command,
        cwd,
        environment,
        purpose="Cargo nested-target build",
        on_line=emit_line,
    )


def _positive_float(value: str) -> float:
    """Parse a positive number of seconds for timeout controls."""
    try:
        seconds = float(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("must be a number of seconds") from exc
    if seconds <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return seconds


if __name__ == "__main__":
    raise SystemExit(main(enable_subreaper=True))
