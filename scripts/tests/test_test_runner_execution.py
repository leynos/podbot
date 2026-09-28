"""Verify phase ordering and exit propagation with a fake Cargo executable."""

from __future__ import annotations

import json
import pathlib
import typing as typ
from dataclasses import dataclass

import pytest
import test_runner
import test_runner_nested

from test_runner_fixtures import (
    FakeCargoConfiguration,
    fake_cargo_environment as _fake_cargo_environment,
    workspace_with_sibling_package,
)
from test_runner_models import RunnerError


@pytest.fixture
def cargo_command_reader(
    tmp_path: pathlib.Path,
) -> typ.Callable[[], list[list[str]]]:
    """Read fake Cargo invocations after each runner execution."""
    command_file = tmp_path / "commands.jsonl"

    def read_commands() -> list[list[str]]:
        if not command_file.exists():
            return []
        return [json.loads(line) for line in command_file.read_text().splitlines()]

    return read_commands


@dataclass
class RunnerExecutionHarness:
    """Bundle filesystem, patch, and process-log fixtures for runner tests."""

    tmp_path: pathlib.Path
    monkeypatch: pytest.MonkeyPatch
    read_commands: typ.Callable[[], list[list[str]]]

    def fake_environment(
        self, configuration: FakeCargoConfiguration = FakeCargoConfiguration()
    ) -> dict[str, str]:
        """Create the configured fake Cargo process for this test."""
        return _fake_cargo_environment(self.tmp_path, self.monkeypatch, configuration)

    def run(self, environment: dict[str, str], cargo_arguments: tuple[str, ...]) -> int:
        """Run the test runner against the harness's fake Cargo executable."""
        return test_runner.main(
            ["--cargo", environment["FAKE_CARGO"], "--", *cargo_arguments]
        )


@pytest.fixture
def runner_harness(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    cargo_command_reader: typ.Callable[[], list[list[str]]],
) -> RunnerExecutionHarness:
    """Group resources shared by process-order and phase-failure tests."""
    return RunnerExecutionHarness(tmp_path, monkeypatch, cargo_command_reader)


@pytest.mark.parametrize(
    ("selection", "target_name"),
    [
        (("--test", "compile_contract"), "compile_contract"),
        (
            ("--no-default-features", "--test", "cli_feature_gating"),
            "cli_feature_gating",
        ),
    ],
    ids=["compile-contract", "no-default-cli-boundary"],
)
def test_cargo_build_exits_before_nested_test_process_starts(
    runner_harness: RunnerExecutionHarness,
    selection: tuple[str, ...],
    target_name: str,
) -> None:
    """Each trybuild executable starts after its current Cargo build exits."""
    environment = runner_harness.fake_environment()
    runner_harness.monkeypatch.setenv("FAKE_REQUIRE_BUILD_RETURNED", "true")
    real_build = test_runner_nested._run_json_build

    def mark_build_complete(*args: typ.Any, **kwargs: typ.Any) -> int:
        status = real_build(*args, **kwargs)
        pathlib.Path(environment["FAKE_BUILD_RETURNED"]).touch()
        return status

    runner_harness.monkeypatch.setattr(
        test_runner_nested, "_run_json_build", mark_build_complete
    )

    status = test_runner.main(["--cargo", environment["FAKE_CARGO"], "--", *selection])

    assert status == 0, "a successful no-run build and direct test should pass"
    invocation = json.loads(pathlib.Path(environment["FAKE_TEST_ARGS"]).read_text())
    assert invocation == [], "the test binary should receive no unexpected arguments"
    commands = runner_harness.read_commands()
    build = next(command for command in commands if "--no-run" in command)
    assert build.index("--test") < build.index(
        "--message-format=json-render-diagnostics"
    ), "the nested target selector must precede Cargo's JSON output option"
    assert build[build.index("--package") + 1] == "podbot", (
        "the nested target build must be scoped to its owning package"
    )
    assert build[build.index("--test") + 1] == target_name, (
        "the isolated Cargo build must select the requested trybuild target"
    )
    assert not any(
        "--test" in command
        and command[command.index("--test") + 1] == target_name
        and "--no-run" not in command
        for command in commands
    ), "no trybuild target may execute inside an outer Cargo test process"


def test_filters_and_harness_arguments_reach_the_current_artifact(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Cargo's positional test filter and post-separator flags reach libtest."""
    environment = _fake_cargo_environment(tmp_path, monkeypatch)

    status = test_runner.main(
        [
            "--cargo",
            environment["FAKE_CARGO"],
            "--",
            "--test",
            "compile_contract",
            "stable_exec_context_signatures_compile",
            "--",
            "--exact",
            "--nocapture",
        ]
    )

    assert status == 0, "the direct compile-contract test should succeed"
    invocation = json.loads(pathlib.Path(environment["FAKE_TEST_ARGS"]).read_text())
    assert invocation == [
        "stable_exec_context_signatures_compile",
        "--exact",
        "--nocapture",
    ], "Cargo filters and harness arguments must reach the direct test binary"


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


@pytest.mark.parametrize(
    ("build_exit", "test_exit", "expected"),
    [(19, 0, 19), (0, 17, 17)],
    ids=["build-failure", "test-failure"],
)
def test_build_and_direct_test_failures_propagate(
    runner_harness: RunnerExecutionHarness,
    build_exit: int,
    test_exit: int,
    expected: int,
) -> None:
    """Neither a Cargo build error nor the direct harness failure is hidden."""
    environment = runner_harness.fake_environment(
        FakeCargoConfiguration(build_exit=build_exit, test_exit=test_exit),
    )

    status = runner_harness.run(environment, ("--test", "compile_contract"))

    assert status == expected, "build and test failures must preserve their exit code"
    test_args = pathlib.Path(environment["FAKE_TEST_ARGS"])
    assert test_args.exists() is (build_exit == 0), (
        "the test binary must only run after a successful build"
    )


def test_artifact_selection_error_returns_runner_failure(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    """A malformed current-build artifact fails through the runner contract."""
    environment = _fake_cargo_environment(tmp_path, monkeypatch)

    def reject_artifacts(*args: typ.Any, **kwargs: typ.Any) -> typ.NoReturn:
        raise RunnerError("no current test artifact")

    monkeypatch.setattr(test_runner_nested, "select_test_executables", reject_artifacts)
    status = test_runner.main(
        ["--cargo", environment["FAKE_CARGO"], "--", "--test", "compile_contract"]
    )

    assert status == 2, "artifact selection errors must return the runner error status"
    assert "no current test artifact" in capsys.readouterr().err, (
        "artifact selection failures must be visible on stderr"
    )


@pytest.mark.parametrize(
    ("no_fail_fast", "nested_phase_runs"),
    [(False, False), (True, True)],
    ids=["fail-fast", "no-fail-fast"],
)
def test_ordinary_failure_obeys_fail_fast_policy(
    runner_harness: RunnerExecutionHarness,
    no_fail_fast: bool,
    nested_phase_runs: bool,
) -> None:
    """Ordinary failures follow the selected Cargo fail-fast policy."""
    environment = runner_harness.fake_environment(
        FakeCargoConfiguration(ordinary_exit=23)
    )
    arguments = (
        ("--all-targets", "--no-fail-fast") if no_fail_fast else ("--all-targets",)
    )

    status = test_runner.main(["--cargo", environment["FAKE_CARGO"], "--", *arguments])

    commands = runner_harness.read_commands()
    assert status == 23, "the ordinary test failure must remain the final status"
    assert any("--no-run" in command for command in commands) is nested_phase_runs, (
        "fail-fast policy must control whether nested-Cargo tests are built"
    )


def test_doctest_failure_does_not_report_completed_ordinary_phase_as_skipped(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    """Fail-fast diagnostics list only phases that remain unstarted."""
    environment = _fake_cargo_environment(
        tmp_path, monkeypatch, FakeCargoConfiguration(doctest_exit=29)
    )

    status = test_runner.main(["--cargo", environment["FAKE_CARGO"], "--"])

    output = capsys.readouterr().out
    assert status == 29, "the doctest failure must remain the runner status"
    assert "remaining packages:" not in output, (
        "completed ordinary package phases must not be reported as skipped"
    )
    assert "Skipping nested-Cargo tests" in output, (
        "the not-yet-started nested phase must be reported"
    )


def test_fail_fast_reports_unstarted_workspace_package(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    cargo_command_reader: typ.Callable[[], list[list[str]]],
    capsys: pytest.CaptureFixture[str],
) -> None:
    """A failure names ordinary package phases that fail-fast skips."""
    metadata = workspace_with_sibling_package(tmp_path)
    environment = _fake_cargo_environment(
        tmp_path,
        monkeypatch,
        FakeCargoConfiguration(ordinary_exit=23, metadata=metadata),
    )

    status = test_runner.main(
        ["--cargo", environment["FAKE_CARGO"], "--", "--workspace", "--all-targets"]
    )

    commands = cargo_command_reader()
    output = capsys.readouterr().out
    assert status == 23, "the first ordinary failure must be returned"
    assert "remaining packages: sibling" in output, (
        "fail-fast diagnostics must identify the skipped sibling package"
    )
    assert not any("sibling" in command for command in commands), (
        "fail-fast must not start later package phases"
    )
