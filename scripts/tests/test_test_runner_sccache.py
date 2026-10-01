"""Test best-effort sccache startup before child process supervision."""

from __future__ import annotations

import subprocess

import pytest
import test_runner_sccache as sccache_module


@pytest.mark.parametrize(
    ("wrapper", "expected"),
    [
        ("/usr/bin/sccache", ("/usr/bin/sccache", "--start-server")),
        (
            "/opt/cache/sccache --cache-size 2G",
            ("/opt/cache/sccache", "--cache-size", "2G", "--start-server"),
        ),
        ("ccache", None),
        ("", None),
    ],
    ids=["path", "arguments", "other-wrapper", "unset"],
)
def test_sccache_start_command_detects_only_sccache_wrappers(
    wrapper: str, expected: tuple[str, ...] | None
) -> None:
    """Start-server is added only when Cargo names sccache as its wrapper."""
    assert (
        sccache_module.configured_sccache_start_command({"RUSTC_WRAPPER": wrapper})
        == expected
    )


@pytest.mark.parametrize(
    "failure",
    [OSError("sccache is missing"), subprocess.TimeoutExpired("sccache", 5)],
    ids=["missing", "unresponsive"],
)
def test_sccache_start_failures_are_best_effort(
    monkeypatch: pytest.MonkeyPatch, failure: Exception
) -> None:
    """Missing or unresponsive sccache startup never blocks the test runner."""

    def raise_failure(*_args: object, **_kwargs: object) -> None:
        raise failure

    monkeypatch.setattr(sccache_module.subprocess, "run", raise_failure)

    sccache_module.start_configured_sccache({"RUSTC_WRAPPER": "sccache"})
