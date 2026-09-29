"""The cache server starts with diagnostics enabled and reports safely."""

from __future__ import annotations

import pathlib

import pytest
import report_sccache_errors
from report_sccache_errors import MAX_DIAGNOSTIC_LINES
from report_sccache_errors import sanitize_error_log
from workflow_contracts import of_type
from workflow_contracts import parse as parse_workflow
from workflow_coverage import cache_reports

SETUP_RUST_ACTION = "leynos/shared-actions/.github/actions/setup-rust"
ERROR_LOG_PATH = "${{ runner.temp }}/sccache-error.log"
DIAGNOSTIC_COMMAND = 'python3 scripts/report_sccache_errors.py "$SCCACHE_ERROR_LOG"'
HEALTH_CHECK_COMMAND = (
    "python3 scripts/check_sccache_health.py --expect-location ghac sccache-stats.json"
)


def test_setup_enables_logging_and_collects_sanitized_diagnostics(
    workflow_texts: dict[str, str],
) -> None:
    """Both cache workflows collect bounded diagnostics after health checks."""
    for workflow in ("ci.yml", "coverage-main.yml"):
        (report,) = cache_reports({workflow: workflow_texts[workflow]})
        document = parse_workflow(workflow, workflow_texts[workflow])
        job = of_type(of_type(document.get("jobs"), dict).get(report.job), dict)
        steps = [of_type(step, dict) for step in of_type(job.get("steps"), list)]
        setup_index = next(
            index
            for index, step in enumerate(steps)
            if str(step.get("uses", "")).partition("@")[0] == SETUP_RUST_ACTION
        )
        setup_env = of_type(steps[setup_index].get("env"), dict)
        assert setup_env == {
            "SCCACHE_LOG": "debug",
            "SCCACHE_ERROR_LOG": ERROR_LOG_PATH,
        }, f"{workflow}: setup-rust must receive the diagnostic settings"
        assert setup_index < report.coverage_index, (
            f"{workflow}: setup-rust must precede coverage compilation"
        )

        diagnostic_index = next(
            index
            for index, step in enumerate(steps)
            if step.get("run") == DIAGNOSTIC_COMMAND
        )
        health_index = next(
            index
            for index, step in enumerate(steps)
            if step.get("run") == HEALTH_CHECK_COMMAND
        )
        diagnostic = steps[diagnostic_index]
        assert diagnostic_index > health_index, (
            f"{workflow}: diagnostics must follow the cache health check"
        )
        assert diagnostic.get("if") == "always()", (
            f"{workflow}: partial write errors must be reported on green jobs too"
        )
        assert diagnostic.get("continue-on-error") is True, (
            f"{workflow}: diagnostic failure must not replace the original result"
        )
        assert of_type(diagnostic.get("env"), dict).get("SCCACHE_ERROR_LOG") == (
            ERROR_LOG_PATH
        ), f"{workflow}: the diagnostic must read this job's server log"
        raw_uploads = [
            step
            for step in steps
            if str(step.get("uses", "")).partition("@")[0] == "actions/upload-artifact"
            and "sccache-error.log"
            in str(of_type(step.get("with"), dict).get("path", ""))
        ]
        assert not raw_uploads, f"{workflow}: never upload the raw sccache log"


def test_write_error_output_redacts_credentials_and_caps_lines() -> None:
    """Keep the backend status while removing secrets and limiting output."""
    log = (
        "ERROR failed to write cache: HTTP 429 from "
        "https://user:password@cache.example/item?token=url-secret "
        "ACTIONS_RUNTIME_TOKEN=runtime-secret"
    )
    lines = sanitize_error_log(log, ("runtime-secret",))

    assert len(lines) == 1, "report the first write error"
    assert "HTTP 429" in lines[0], "retain an available backend status"
    assert "cache.example" in lines[0], "retain the backend host"
    assert all(
        secret not in lines[0]
        for secret in (
            "user:password",
            "/item",
            "?token=",
            "url-secret",
            "runtime-secret",
        )
    ), "redact URL paths, query credentials and runtime tokens"
    repeated_errors = "\n".join("ERROR failed to write cache" for _ in range(20))
    assert len(sanitize_error_log(repeated_errors)) <= MAX_DIAGNOSTIC_LINES, (
        "cap the number of emitted lines"
    )


def test_diagnostic_reads_only_the_bounded_log_prefix(
    tmp_path: pathlib.Path, monkeypatch
) -> None:
    """A large server log cannot cause an unbounded diagnostic read."""
    monkeypatch.setattr(report_sccache_errors, "MAX_LOG_BYTES", 10)
    log_file = tmp_path / "sccache-error.log"
    log_file.write_bytes(b"safe prefixERROR failed to write HTTP 429")

    lines = report_sccache_errors._read_diagnostic(log_file, ())

    assert lines == (
        "no write-related error found in the inspected log prefix (truncated)",
    ), "do not inspect or report log entries beyond the configured byte limit"


def test_empty_error_log_environment_is_treated_as_unavailable(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """An empty optional path reaches the helper's unavailable-file message."""
    monkeypatch.setenv("SCCACHE_ERROR_LOG", "")

    assert report_sccache_errors.parse_arguments([]).log_file is None
