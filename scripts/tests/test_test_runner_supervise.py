"""Check the one-off supervision entry point and its exit semantics."""

from __future__ import annotations

import sys

import pytest

import test_runner


def test_supervise_mode_propagates_command_failure() -> None:
    """The generic mode returns the exact child failure status."""
    status = test_runner.main(
        [
            "--supervise",
            "--timeout",
            "5",
            "--",
            sys.executable,
            "-c",
            "raise SystemExit(23)",
        ]
    )

    assert status == 23, "supervise mode must preserve ordinary child failures"


def test_supervise_mode_bounds_a_stalled_command(
    capsys: pytest.CaptureFixture[str],
) -> None:
    """A timed-out generic command uses the runner's timeout exit code."""
    status = test_runner.main(
        [
            "--supervise",
            "--timeout",
            "0.1",
            "--watch-interval",
            "0.05",
            "--",
            sys.executable,
            "-c",
            "import time; time.sleep(5)",
        ],
        enable_subreaper=True,
    )

    assert status == 124, "a stalled command must return the distinct timeout status"
    assert "test runner: timed out" in capsys.readouterr().err, (
        "timeout diagnostics must identify the bounded failure"
    )


def test_supervise_mode_requires_a_command(capsys: pytest.CaptureFixture[str]) -> None:
    """An empty supervised command fails with a direct usage diagnostic."""
    status = test_runner.main(["--supervise", "--"])

    assert status == 2, "an absent child command must be rejected"
    assert "requires a command after `--`" in capsys.readouterr().err
