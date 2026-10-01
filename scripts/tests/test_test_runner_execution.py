"""Verify ordinary Cargo phases and workspace command construction."""

from __future__ import annotations

import pathlib
import types
import typing as typ

import pytest
import test_runner
import test_runner_phases

from test_runner_fixtures import (
    FakeCargoConfiguration,
    fake_cargo_environment as _fake_cargo_environment,
    package_document,
    workspace_with_sibling_package,
)
from test_runner_options import parse_cargo_test_options
from test_runner_plan import create_test_plan


def test_ordinary_cargo_phase_uses_the_planned_workspace_root(
    tmp_path: pathlib.Path,
) -> None:
    """The ordinary phase uses its plan for command and process inputs."""
    caller_directory = tmp_path / "caller"
    workspace_root = tmp_path / "workspace"
    environment = {"CARGO_HOME": str(tmp_path / "cargo-home")}
    captured_requests = []

    def run_inherited(request: typ.Any) -> int:
        captured_requests.append(request)
        return 23

    context = types.SimpleNamespace(
        cargo_command=("cargo",),
        cwd=caller_directory,
        environment=environment,
        supervisor=types.SimpleNamespace(
            run_inherited=run_inherited,
            terminal_status=None,
        ),
    )
    options = parse_cargo_test_options(
        [
            "--package",
            "podbot",
            "--features",
            "internal",
            "--lib",
            "selected_test",
            "--",
            "--exact",
            "--nocapture",
        ],
        cwd=caller_directory,
    )
    plan = create_test_plan(package_document(workspace_root), options)

    phase_statuses, should_stop = test_runner_phases._run_ordinary_phase(context, plan)

    assert phase_statuses == (23,), "the supervisor status must pass through the phase"
    assert should_stop, "ordinary failure must retain Cargo's fail-fast policy"
    request = captured_requests[0]
    assert request.command == [
        "cargo",
        "test",
        "--features",
        "internal",
        "--package",
        "podbot",
        "--lib",
        "selected_test",
        "--",
        "--exact",
        "--nocapture",
    ], "ordinary command order must preserve features, filter, and harness flags"
    assert request.cwd == plan.workspace_root, (
        "ordinary Cargo tests must run from the metadata workspace root"
    )
    assert context.cwd != plan.workspace_root, (
        "the planned workspace root must remain distinct from caller cwd"
    )
    assert request.environment is environment, (
        "ordinary Cargo tests must receive the caller's environment"
    )


def test_doctest_phase_preserves_filters_and_harness_arguments(
    tmp_path: pathlib.Path,
) -> None:
    """The doctest phase uses its plan and forwards Cargo and harness inputs."""
    caller_directory = tmp_path / "caller"
    workspace_root = tmp_path / "workspace"
    environment = {"CARGO_HOME": str(tmp_path / "cargo-home")}
    captured_requests = []

    def run_inherited(request: typ.Any) -> int:
        captured_requests.append(request)
        return 29

    context = types.SimpleNamespace(
        cargo_command=("cargo",),
        cwd=caller_directory,
        environment=environment,
        supervisor=types.SimpleNamespace(run_inherited=run_inherited),
    )
    options = parse_cargo_test_options(
        ["--doc", "documentation_case", "--", "--exact", "--nocapture"],
        cwd=caller_directory,
    )
    plan = create_test_plan(package_document(workspace_root), options)

    phase_statuses, should_stop = test_runner_phases._run_doctest_phase(context, plan)

    assert phase_statuses == (29,), "the supervisor status must pass through the phase"
    assert should_stop, "doctest failure must retain Cargo's fail-fast policy"
    request = captured_requests[0]
    assert request.command == [
        "cargo",
        "test",
        "--doc",
        "documentation_case",
        "--",
        "--exact",
        "--nocapture",
    ], "doctest command order must preserve filter and harness flags"
    assert request.cwd == plan.workspace_root, (
        "doctests must run from the metadata workspace root"
    )
    assert context.cwd != plan.workspace_root, (
        "the planned workspace root must remain distinct from caller cwd"
    )
    assert request.environment is environment, (
        "doctests must receive the caller's environment"
    )


@pytest.mark.parametrize(
    "selector",
    [
        ("--package=podbot",),
        ("--package", "podbot"),
        ("-ppodbot",),
        ("-p", "podbot"),
    ],
)
def test_attached_package_selectors_are_removed_before_per_package_commands(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    cargo_command_reader: typ.Callable[[], list[list[str]]],
    selector: tuple[str, ...],
) -> None:
    """Package filter spellings do not duplicate package-scoped phases."""
    environment = _fake_cargo_environment(tmp_path, monkeypatch)

    status = test_runner.main(
        [
            "--cargo",
            environment["FAKE_CARGO"],
            "--",
            "--all-targets",
            *selector,
        ]
    )

    commands = cargo_command_reader()
    assert status == 0, "a valid attached package selector must preserve test success"
    assert all(command.count("--package") == 1 for command in commands), (
        "each per-package Cargo phase must receive exactly one package selector"
    )


def test_excluded_workspace_packages_are_removed_from_phase_commands(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    cargo_command_reader: typ.Callable[[], list[list[str]]],
) -> None:
    """Expanded workspace excludes do not leak into per-package commands."""
    metadata = workspace_with_sibling_package(tmp_path)
    environment = _fake_cargo_environment(
        tmp_path, monkeypatch, FakeCargoConfiguration(metadata=metadata)
    )

    status = test_runner.main(
        [
            "--cargo",
            environment["FAKE_CARGO"],
            "--",
            "--workspace",
            "--exclude",
            "sibling",
            "--all-targets",
        ]
    )

    commands = cargo_command_reader()
    assert status == 0, "a valid workspace exclude must preserve test success"
    assert all("--exclude" not in command for command in commands), (
        "metadata-expanded workspace excludes must be removed from each phase"
    )
    assert all("sibling" not in command for command in commands), (
        "excluded workspace packages must not get their own test phase"
    )
