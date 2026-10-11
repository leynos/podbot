"""Verify runner exit propagation and Cargo fail-fast behavior."""

from __future__ import annotations

import pathlib
import typing as typ
from types import SimpleNamespace

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
    "message",
    [
        pytest.param("the Cargo command is empty", id="ordinary-error"),
        pytest.param(
            "failed to read https://user:url-secret@example.test/private?token=abc",
            id="credential-url",
        ),
        pytest.param("long diagnostic " + "x" * 1200, id="bounded-long-error"),
    ],
)
def test_main_redacts_and_bounds_runner_errors(
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    message: str,
) -> None:
    """Top-level runner errors preserve safe text and redact sensitive values."""
    secret = "known-environment-secret"
    monkeypatch.setattr(
        test_runner,
        "os",
        SimpleNamespace(environ={"GITHUB_TOKEN": secret}),
    )

    def fail_dispatch(*args: typ.Any, **kwargs: typ.Any) -> typ.NoReturn:
        raise RunnerError(message)

    monkeypatch.setattr(test_runner, "_run_parsed_arguments", fail_dispatch)

    status = test_runner.main([])

    error = capsys.readouterr().err
    diagnostic = error.removeprefix("test runner: ").rstrip("\n")
    assert status == 2, "runner errors must preserve the established exit status"
    assert secret not in error, "known environment secrets must not reach stderr"
    if message == "the Cargo command is empty":
        assert error == "test runner: the Cargo command is empty\n", (
            "ordinary runner error wording must remain unchanged"
        )
    elif "url-secret" in message:
        assert "<URL https://example.test>" in error, (
            "credential-bearing URLs must retain only their safe authority"
        )
        assert "url-secret" not in error and "?token=abc" not in error, (
            "URL credentials and paths must not reach stderr"
        )
    else:
        assert len(diagnostic) <= 1000, "runner errors must obey the text bound"
        assert diagnostic.endswith("... [truncated]"), (
            "long runner errors must disclose that their text was truncated"
        )


@pytest.mark.parametrize(
    "case",
    [
        pytest.param(
            (
                "--token=known-environment-secret",
                {"GITHUB_TOKEN": "known-environment-secret"},
                ("known-environment-secret",),
                False,
            ),
            id="credential-option",
        ),
        pytest.param(
            (
                "--custom=known-environment-secret",
                {"SERVICE_API_KEY": "known-environment-secret"},
                ("known-environment-secret",),
                False,
            ),
            id="environment-secret",
        ),
        pytest.param(
            (
                "--custom=https://user:url-secret@example.test/private?token=query-secret",
                {},
                ("user", "url-secret", "private", "query-secret"),
                False,
            ),
            id="credential-bearing-url",
        ),
        pytest.param(
            ("--custom=" + "x" * 1200, {}, (), True),
            id="bounded-long-error",
        ),
    ],
)
def test_parser_errors_redact_sensitive_argument_values(
    capsys: pytest.CaptureFixture[str],
    case: tuple[str, dict[str, str], tuple[str, ...], bool],
) -> None:
    """Argparse diagnostics redact secrets and retain their existing status."""
    argument, environment, secrets, is_long = case

    with pytest.raises(SystemExit) as exit_info:
        test_runner._parse_arguments([argument], environment)

    error = capsys.readouterr().err
    assert exit_info.value.code == 2, "invalid runner options must keep status 2"
    assert "error: unrecognized arguments:" in error, (
        "parser failures must retain their ordinary diagnostic heading"
    )
    assert all(secret not in error for secret in secrets), (
        "sensitive unknown-argument values must not reach stderr"
    )
    if is_long:
        assert len(error.split("error: ", maxsplit=1)[1].rstrip("\n")) <= 1000, (
            "parser diagnostics must obey the existing text bound"
        )
        assert error.rstrip().endswith("... [truncated]"), (
            "long parser diagnostics must disclose truncation"
        )


def test_parser_error_preserves_ordinary_wording_and_exit_status(
    capsys: pytest.CaptureFixture[str],
) -> None:
    """Ordinary argparse errors keep their text and exit status."""
    with pytest.raises(SystemExit) as exit_info:
        test_runner._parse_arguments(["--ordinary-unknown"], {})

    error = capsys.readouterr().err
    assert exit_info.value.code == 2, "ordinary parser errors must keep status 2"
    assert error.endswith("error: unrecognized arguments: --ordinary-unknown\n"), (
        "ordinary parser wording must remain unchanged"
    )


def test_parser_help_and_post_separator_arguments_remain_unchanged(
    capsys: pytest.CaptureFixture[str],
) -> None:
    """Help remains available and Cargo arguments remain unredacted."""
    with pytest.raises(SystemExit) as exit_info:
        test_runner._parse_arguments(["--help"], {})

    help_output = capsys.readouterr()
    assert exit_info.value.code == 0, "help must retain its successful exit status"
    assert "usage:" in help_output.out and "--watch-interval" in help_output.out, (
        "help must retain the parser usage and runner controls"
    )
    assert not help_output.err, "help must not add an error diagnostic"

    parsed = test_runner._parse_arguments(["--", "--token=forwarded-secret"], {})
    assert test_runner._cargo_arguments(parsed) == ["--token=forwarded-secret"], (
        "arguments after the separator must be forwarded unchanged"
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
