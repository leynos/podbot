"""Specify metadata failure propagation to the test runner boundary."""

from __future__ import annotations

import pathlib
import typing as typ
from typing import cast

import pytest
import test_runner
import test_runner_cargo
from test_runner_cargo import RunnerCommandFailure
from test_runner_context import TestRunnerContext
from test_runner_options import parse_cargo_test_options
from test_runner_supervisor import CommandRequest, ProcessSupervisor


class FailedMetadataSupervisor:
    """Return a captured metadata failure without rendering it."""

    def __init__(self, terminal_status: int | None) -> None:
        self.terminal_status = terminal_status

    def run_capture(self, request: CommandRequest) -> tuple[int, str, str]:
        del request
        return 17, "", "cargo metadata failed TOKEN=raw-metadata-value\n"


@pytest.mark.parametrize(
    ("terminal_status", "expected_status", "expected_summary"),
    (
        (None, 2, "cargo metadata exited with status 17"),
        (23, 23, None),
    ),
    ids=("cargo-failure", "supervisor-terminal-status"),
)
def test_metadata_failure_is_structured_without_output(
    capsys: pytest.CaptureFixture[str],
    terminal_status: int | None,
    expected_status: int,
    expected_summary: str | None,
) -> None:
    """Metadata discovery retains status and diagnostics without writing."""
    supervisor = FailedMetadataSupervisor(terminal_status)
    context = TestRunnerContext(
        ("cargo",), pathlib.Path.cwd(), {}, cast(ProcessSupervisor, supervisor)
    )
    options = parse_cargo_test_options([], cwd=pathlib.Path.cwd())

    with pytest.raises(RunnerCommandFailure) as raised:
        test_runner_cargo.load_cargo_metadata(context, options)

    assert capsys.readouterr() == ("", ""), "query helpers must not render errors"
    assert raised.value.status == expected_status, "preserve the runner status"
    assert raised.value.diagnostic == (
        "cargo metadata failed TOKEN=raw-metadata-value\n"
    ), "carry captured stderr intact to the reporting boundary"
    assert raised.value.summary == expected_summary, "preserve failure context"


def test_runner_renders_structured_metadata_failure_once(
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The outer runner redacts and emits structured metadata failure once."""
    failure = RunnerCommandFailure(
        2,
        diagnostic="cargo metadata failed TOKEN=raw-metadata-value\n",
        summary="cargo metadata exited with status 17",
    )

    def raise_failure(
        parsed: typ.Any,
        cargo_arguments: list[str],
        enable_subreaper: bool,
    ) -> int:
        del parsed, cargo_arguments, enable_subreaper
        raise failure

    monkeypatch.setattr(test_runner, "_run_cargo_tests", raise_failure)

    status = test_runner.main([])

    assert status == 2, "metadata failures retain the runner's ordinary status"
    assert capsys.readouterr().err == (
        "cargo metadata failed TOKEN=<redacted>\n"
        "test runner: cargo metadata exited with status 17\n"
    ), "render captured stderr and summary once at the outer boundary"
