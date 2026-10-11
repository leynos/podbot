"""Exercise the sanitized sccache diagnostic command-line boundary."""

from __future__ import annotations

import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

import pytest

from report_sccache_errors import MAX_DIAGNOSTIC_LINES, MAX_LINE_LENGTH


_SCCACHE_SECRET = "arbitrary-sccache-environment-secret"


@dataclass(frozen=True)
class ReporterScenario:
    """Describe one optional reporter log and its output contract."""

    log_contents: str | None
    expected_stdout: str | None
    expected_fragments: tuple[str, ...] = ()


@pytest.mark.parametrize(
    "scenario",
    (
        pytest.param(
            ReporterScenario(
                log_contents="\n".join(
                    f"ERROR failed to write cache: TOKEN={_SCCACHE_SECRET}"
                    for _ in range(MAX_DIAGNOSTIC_LINES + 4)
                ),
                expected_stdout=None,
                expected_fragments=(
                    "first sccache backend write failure",
                    "TOKEN=<redacted>",
                ),
            ),
            id="sanitized-write-failure",
        ),
        pytest.param(
            ReporterScenario(
                log_contents=None,
                expected_stdout="sccache: sccache error log is unavailable\n",
            ),
            id="missing-log",
        ),
        pytest.param(
            ReporterScenario(
                log_contents="",
                expected_stdout=(
                    "sccache: no backend write failure found in the bounded error log\n"
                ),
            ),
            id="empty-log",
        ),
    ),
)
def test_reporter_cli_prints_bounded_sanitized_output_and_succeeds(
    tmp_path: Path, scenario: ReporterScenario
) -> None:
    """Every diagnostic outcome is prefixed, bounded, and non-failing."""
    secret = _SCCACHE_SECRET
    log_file = tmp_path / "sccache-error.log"
    if scenario.log_contents is not None:
        log_file.write_text(scenario.log_contents, encoding="utf-8")

    repository = Path(__file__).resolve().parents[2]
    environment = {
        "PATH": os.environ.get("PATH", ""),
        "SCCACHE_ERROR_LOG": str(log_file),
        "SCCACHE_TOKEN": secret,
    }
    result = subprocess.run(
        [sys.executable, str(repository / "scripts/report_sccache_errors.py")],
        cwd=repository,
        env=environment,
        capture_output=True,
        check=False,
        text=True,
    )

    lines = result.stdout.splitlines()
    assert result.returncode == 0, "diagnostic output must not alter workflow status"
    assert not result.stderr, "the CLI must keep its diagnostics on stdout"
    assert lines and all(line.startswith("sccache: ") for line in lines), (
        "every output line must be namespaced against workflow commands"
    )
    assert len(lines) <= MAX_DIAGNOSTIC_LINES + 1, "bound the number of lines"
    assert all(len(line) <= MAX_LINE_LENGTH for line in lines), (
        "bound each diagnostic line"
    )
    assert secret not in result.stdout, "environment secrets must not be exposed"
    if scenario.expected_stdout is not None:
        assert result.stdout == scenario.expected_stdout, (
            "missing and empty logs must have stable diagnostics"
        )
    assert all(fragment in result.stdout for fragment in scenario.expected_fragments), (
        "report bounded backend failures while redacting environment secrets"
    )
