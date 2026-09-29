"""Run `make fmt` and `make check-fmt` against recording stubs.

The structural contract reads the Makefile; this one asks what Make actually
runs. Every tool the two targets call is replaced by a stub on `PATH` that
records its arguments, so a real run in a temporary Git repository proves the
tools are invoked with the estate's flags, in the right order, and that a
failing tool fails the target. Nothing in the real repository is rewritten.
"""

from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

import pytest
from markdown_wiring_rules import ESTATE_FLAGS

ROOT = Path(__file__).resolve().parents[2]
STUBBED = ("cargo", "mdtablefix", "markdownlint-cli2")
MAKE = shutil.which("make")
TIMEOUT = 120


@pytest.fixture
def sandbox(tmp_path: Path) -> Path:
    """Return a temporary Git repository holding the real Makefile and stubs.

    Each stub appends its name and arguments to `calls.log` and exits with the
    status named in `<tool>.status` beside it, or zero.
    """
    repo = tmp_path / "repo"
    repo.mkdir()
    shutil.copy(ROOT / "Makefile", repo / "Makefile")
    (repo / "README.md").write_text("# Sandbox\n", encoding="utf-8")
    subprocess.run(["git", "init", "-q"], cwd=repo, check=True)  # noqa: S607
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    log = tmp_path / "calls.log"
    for tool in STUBBED:
        stub = bin_dir / tool
        script = (
            f'#!/bin/sh\nprintf "%s %s\\n" "{tool}" "$*" >> "{log}"\n'
            f'status="{bin_dir}/{tool}.status"\n'
            'if [ -f "$status" ]; then exit "$(cat "$status")"; fi\nexit 0\n'
        )
        stub.write_text(script, encoding="utf-8")
        stub.chmod(0o755)
    return tmp_path


def _run(sandbox: Path, target: str) -> tuple[int, list[str]]:
    """Run ``target`` in the sandbox with the stubs first on `PATH`.

    Returns the exit status and the recorded tool invocations, in order.
    """
    assert MAKE, "make must be installed to run these tests"
    env = {**os.environ, "PATH": f"{sandbox / 'bin'}{os.pathsep}{os.environ['PATH']}"}
    result = subprocess.run(  # noqa: S603
        [MAKE, target],
        cwd=sandbox / "repo",
        env=env,
        capture_output=True,
        text=True,
        check=False,
        timeout=TIMEOUT,
    )
    log = sandbox / "calls.log"
    calls = log.read_text(encoding="utf-8").splitlines() if log.exists() else []
    return result.returncode, calls


def _tool_calls(calls: list[str], tool: str) -> list[str]:
    """Return the recorded invocations of ``tool``."""
    return [call for call in calls if call.split()[0] == tool]


def test_check_fmt_runs_the_table_check_with_every_estate_flag(sandbox: Path) -> None:
    """`make check-fmt` calls `mdtablefix --check` with all seven flags."""
    status, calls = _run(sandbox, "check-fmt")
    checks = [c for c in _tool_calls(calls, "mdtablefix") if "--check" in c]
    assert status == 0, calls
    assert len(checks) == 1, calls
    assert all(flag in checks[0].split() for flag in ESTATE_FLAGS), checks


def test_fmt_rewrites_and_then_lints_with_every_estate_flag(sandbox: Path) -> None:
    """`make fmt` runs the rewrite, then the linter's fix, in that order."""
    status, calls = _run(sandbox, "fmt")
    tools = [call.split()[0] for call in calls]
    rewrite = [c for c in _tool_calls(calls, "mdtablefix") if "--in-place" in c]
    lint = [c for c in _tool_calls(calls, "markdownlint-cli2") if "--fix" in c]
    assert status == 0, calls
    assert rewrite
    assert lint
    assert all(flag in rewrite[0].split() for flag in ESTATE_FLAGS), rewrite
    assert tools.index("mdtablefix") < tools.index("markdownlint-cli2"), calls


@pytest.mark.parametrize(
    ("target", "tool"),
    [("check-fmt", "mdtablefix"), ("fmt", "mdtablefix"), ("fmt", "markdownlint-cli2")],
)
def test_a_failing_tool_fails_the_target(sandbox: Path, target: str, tool: str) -> None:
    """The tool's exit status reaches Make, so a red check cannot go green."""
    (sandbox / "bin" / f"{tool}.status").write_text("7", encoding="utf-8")
    status, calls = _run(sandbox, target)
    assert status != 0, calls
    assert _tool_calls(calls, tool), calls
