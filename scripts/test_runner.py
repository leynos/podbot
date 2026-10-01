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
import math
import os
import pathlib
import shlex
import sys

from test_runner_cargo import RunnerCommandFailure, load_cargo_metadata
from test_runner_models import RunnerError
from test_runner_options import parse_cargo_test_options
from test_runner_plan import create_test_plan
from test_runner_context import TestRunnerContext
from test_runner_phases import run_test_plan
from test_runner_supervise import supervise_command
from test_runner_sccache import start_configured_sccache
from test_runner_supervisor import ProcessSupervisor


def main(arguments: list[str] | None = None, *, enable_subreaper: bool = False) -> int:
    """Run Cargo tests using the repository's nested-target registry.

    Examples
    --------
    >>> main(["--cargo", "false", "--", "--bad-option"]) != 0
    True
    """
    parsed = _parse_arguments(arguments)
    try:
        return _run_parsed_arguments(parsed, enable_subreaper)
    except RunnerCommandFailure as exc:
        return exc.status
    except (OSError, RunnerError) as exc:
        print(f"test runner: {exc}", file=sys.stderr)
        return 2


def _parse_arguments(arguments: list[str] | None) -> argparse.Namespace:
    """Parse runner controls separately from Cargo's test arguments."""
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
    parser.add_argument(
        "--supervise",
        action="store_true",
        help="run the command after `--` with bounded process supervision",
    )
    parser.add_argument("cargo_arguments", nargs=argparse.REMAINDER)
    return parser.parse_args(arguments)


def _cargo_arguments(parsed: argparse.Namespace) -> list[str]:
    """Remove the separator consumed by the runner from Cargo's arguments."""
    arguments = list(parsed.cargo_arguments)
    if arguments and arguments[0] == "--":
        arguments.pop(0)
    return arguments


def _run_parsed_arguments(
    parsed: argparse.Namespace,
    enable_subreaper: bool,
) -> int:
    """Dispatch supervised commands or the phased Cargo test workflow."""
    cargo_arguments = _cargo_arguments(parsed)
    if parsed.supervise:
        return supervise_command(
            cargo_arguments,
            parsed.timeout,
            parsed.watch_interval,
            enable_subreaper=enable_subreaper,
        )
    return _run_cargo_tests(parsed, cargo_arguments, enable_subreaper)


def _run_cargo_tests(
    parsed: argparse.Namespace,
    cargo_arguments: list[str],
    enable_subreaper: bool,
) -> int:
    """Load Cargo metadata, build the phase plan, and run its test phases."""
    cargo_command = tuple(shlex.split(parsed.cargo))
    if not cargo_command:
        raise RunnerError("the Cargo command is empty")
    cwd = pathlib.Path.cwd().resolve()
    options = parse_cargo_test_options(cargo_arguments, cwd=cwd)
    environment = os.environ.copy()
    start_configured_sccache(environment)
    with ProcessSupervisor(
        parsed.timeout,
        parsed.watch_interval,
        enable_subreaper=enable_subreaper,
    ) as supervisor:
        context = TestRunnerContext(cargo_command, cwd, environment, supervisor)
        metadata = load_cargo_metadata(context, options)
        plan = create_test_plan(metadata, options)
        environment["CARGO_TARGET_DIR"] = str(plan.target_directory)
        context = TestRunnerContext(
            cargo_command, plan.workspace_root, environment, supervisor
        )
        return run_test_plan(context, plan)


def _positive_float(value: str) -> float:
    """Parse a positive number of seconds for timeout controls."""
    try:
        seconds = float(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("must be a number of seconds") from exc
    if not math.isfinite(seconds):
        raise argparse.ArgumentTypeError("must be a finite number of seconds")
    if seconds <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return seconds


if __name__ == "__main__":
    raise SystemExit(main(enable_subreaper=True))
