"""Verify runner exit propagation and Cargo fail-fast behavior."""

from __future__ import annotations

import pathlib
import typing as typ

import pytest
import test_runner
from conftest import RunnerExecutionHarness
import test_runner_nested

from test_runner_fixtures import (
    FakeCargoConfiguration,
    fake_cargo_environment as _fake_cargo_environment,
    workspace_with_sibling_package,
)
from test_runner_models import RunnerError


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
