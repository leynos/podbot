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


@pytest.mark.parametrize(
    ("phase", "cargo_arguments", "supervisor_status", "expected_command"),
    [
        pytest.param(
            test_runner_phases._run_ordinary_phase,
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
            23,
            [
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
            ],
            id="ordinary",
        ),
        pytest.param(
            test_runner_phases._run_doctest_phase,
            ["--doc", "documentation_case", "--", "--exact", "--nocapture"],
            29,
            [
                "cargo",
                "test",
                "--doc",
                "documentation_case",
                "--",
                "--exact",
                "--nocapture",
            ],
            id="doctest",
        ),
    ],
)
def test_cargo_phase_preserves_planned_process_inputs(
    phase: typ.Callable[[typ.Any, typ.Any], tuple[tuple[int, ...], bool]],
    cargo_arguments: list[str],
    supervisor_status: int,
    expected_command: list[str],
) -> None:
    """Cargo phases preserve the command and process inputs from their plan."""
    caller_directory = pathlib.Path("caller")
    workspace_root = pathlib.Path("workspace")
    environment = {"CARGO_HOME": "cargo-home"}
    captured_requests = []

    def run_inherited(request: typ.Any) -> int:
        captured_requests.append(request)
        return supervisor_status

    context = types.SimpleNamespace(
        cargo_command=("cargo",),
        cwd=caller_directory,
        environment=environment,
        supervisor=types.SimpleNamespace(
            run_inherited=run_inherited,
            terminal_status=None,
        ),
    )
    options = parse_cargo_test_options(cargo_arguments, cwd=caller_directory)
    plan = create_test_plan(package_document(workspace_root), options)

    phase_statuses, should_stop = phase(context, plan)
    phase_name = phase.__name__.removeprefix("_run_").removesuffix("_phase")

    assert phase_statuses == (supervisor_status,), (
        f"{phase_name} must pass through the supervisor status"
    )
    assert should_stop, f"{phase_name} failure must retain Cargo's fail-fast policy"
    assert len(captured_requests) == 1, (
        f"{phase_name} must capture exactly one supervisor request"
    )
    request = captured_requests[0]
    assert request.command == expected_command, (
        f"{phase_name} command order must preserve Cargo, filter, and harness flags"
    )
    assert request.cwd == plan.workspace_root, (
        f"{phase_name} must run from the metadata workspace root"
    )
    assert context.cwd != plan.workspace_root, (
        f"{phase_name} workspace root must remain distinct from caller cwd"
    )
    assert request.environment is environment, (
        f"{phase_name} must receive the caller's environment"
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
