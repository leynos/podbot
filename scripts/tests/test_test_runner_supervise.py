"""Check the one-off supervision entry point and its exit semantics."""

from __future__ import annotations

import argparse
import sys

import pytest

import test_runner
import test_runner_supervisor


@pytest.mark.parametrize("value", ["nan", "inf", "-inf", "Infinity"])
def test_timeout_parser_rejects_non_finite_values(value: str) -> None:
    """Timeouts must be finite so the shared deadline always expires."""
    with pytest.raises(argparse.ArgumentTypeError, match="finite"):
        test_runner._positive_float(value)


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


def test_process_tree_refresh_is_throttled_and_refreshed_on_exit(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Long-running commands use periodic scans plus one final exit scan."""
    refresh_count = 0
    real_refresh = test_runner_supervisor.OwnedProcessTree.refresh

    def count_refreshes(tree: test_runner_supervisor.OwnedProcessTree) -> dict:
        nonlocal refresh_count
        refresh_count += 1
        return real_refresh(tree)

    monkeypatch.setattr(
        test_runner_supervisor.OwnedProcessTree, "refresh", count_refreshes
    )
    status = test_runner.main(
        [
            "--supervise",
            "--timeout",
            "5",
            "--",
            sys.executable,
            "-c",
            "import time; time.sleep(1.2)",
        ]
    )

    assert status == 0, "tree monitoring must preserve a successful command result"
    assert 2 <= refresh_count <= 4, (
        "tree scanning must be periodic and include a fresh scan when the root exits"
    )


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
    assert "requires a command after `--`" in capsys.readouterr().err, (
        "an empty supervised command must explain the required child arguments"
    )
