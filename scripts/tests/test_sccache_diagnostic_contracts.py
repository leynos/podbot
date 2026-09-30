"""The cache server starts with diagnostics enabled and reports safely."""

from __future__ import annotations

import pathlib
import time
import typing as typ

import pytest
import report_sccache_errors
from report_sccache_errors import MAX_DIAGNOSTIC_LINES
from report_sccache_errors import _line_window
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


def _cache_job_steps(
    workflow: str, workflow_texts: dict[str, str]
) -> tuple[list[dict[str, typ.Any]], int]:
    """Return typed cache-job steps and the workflow's coverage index."""
    (report,) = cache_reports({workflow: workflow_texts[workflow]})
    document = parse_workflow(workflow, workflow_texts[workflow])
    job = of_type(of_type(document.get("jobs"), dict).get(report.job), dict)
    steps = [of_type(step, dict) for step in of_type(job.get("steps"), list)]
    return steps, report.coverage_index


@pytest.mark.parametrize("workflow", ("ci.yml", "coverage-main.yml"))
def test_setup_enables_persistent_sccache_logging(
    workflow: str, workflow_texts: dict[str, str]
) -> None:
    """Configure diagnostics before server startup and coverage compilation."""
    steps, coverage_index = _cache_job_steps(workflow, workflow_texts)
    setup_index = next(
        index
        for index, step in enumerate(steps)
        if str(step.get("uses", "")).partition("@")[0] == SETUP_RUST_ACTION
    )
    configure_index = next(
        index
        for index, step in enumerate(steps)
        if step.get("name") == "Configure sccache diagnostics"
    )
    configure = steps[configure_index]
    assert configure_index < setup_index, (
        f"{workflow}: diagnostic logging must be configured before server startup"
    )
    assert of_type(configure.get("env"), dict) == {
        "SCCACHE_ERROR_LOG": ERROR_LOG_PATH,
    }, f"{workflow}: use this job's runner temporary directory"
    assert str(configure.get("run", "")).splitlines() == [
        "printf 'SCCACHE_LOG=debug\\n' >> \"$GITHUB_ENV\"",
        'printf \'SCCACHE_ERROR_LOG=%s\\n\' "$SCCACHE_ERROR_LOG" >> "$GITHUB_ENV"',
    ], f"{workflow}: both diagnostics must persist for later server restarts"
    assert not steps[setup_index].get("env"), (
        f"{workflow}: setup-rust must inherit job-persisted diagnostic settings"
    )
    assert setup_index < coverage_index, (
        f"{workflow}: setup-rust must precede coverage compilation"
    )


@pytest.mark.parametrize("workflow", ("ci.yml", "coverage-main.yml"))
def test_sccache_diagnostics_follow_health_check_without_exposing_raw_logs(
    workflow: str, workflow_texts: dict[str, str]
) -> None:
    """Report after the health check without exposing the raw server log."""
    steps, _ = _cache_job_steps(workflow, workflow_texts)
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
        and "sccache-error.log" in str(of_type(step.get("with"), dict).get("path", ""))
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


def test_startup_probe_already_exists_does_not_hide_store_failure() -> None:
    """The cache capability probe tolerates an existing sentinel entry."""
    log = "\n".join(
        (
            "WARN opendal::services: path=.sccache_check: write failed "
            "AlreadyExists (permanent), response status: 409",
            "DEBUG opendal::services: path=.sccache_check: write failed",
            "ERROR failed to write cache object: HTTP 429 from "
            "https://cache.example/item?token=secret",
        )
    )

    lines = sanitize_error_log(log, ("secret",))

    assert len(lines) == 1, "skip both probe lines and report the store failure"
    assert "HTTP 429" in lines[0], "retain the actual store's backend status"
    assert ".sccache_check" not in lines[0], "do not promote the probe warning"
    assert "secret" not in lines[0], "keep the real backend URL sanitized"


def test_line_window_includes_all_requested_preceding_lines() -> None:
    """Context windows begin at the first requested full line."""
    log = "first context\nsecond context\nERROR write request failed\nafter\n"

    start, end = _line_window(log, log.index("ERROR"), preceding=2, following=1)

    assert log[start:end] == log, "include both context lines and one following line"


@pytest.mark.parametrize(
    ("text", "line_start", "preceding", "following", "start", "end", "window"),
    [
        pytest.param(
            "first\nselected\nlast\n", 6, 0, 0, 6, 15, "selected\n", id="zero-context"
        ),
        pytest.param(
            "selected\nmiddle\nlast", 0, 2, 0, 0, 9, "selected\n", id="at-start"
        ),
        pytest.param("first\nselected", 6, 0, 1, 6, 14, "selected", id="at-end"),
        pytest.param(
            "one\nselected\n", 4, 9, 9, 0, 13, "one\nselected\n", id="excess-context"
        ),
        pytest.param(
            "before\nlast", 7, 1, 0, 0, 11, "before\nlast", id="no-final-newline"
        ),
        pytest.param("", 0, 0, 0, 0, 0, "", id="empty-text"),
        pytest.param(
            "\n\nselected\n\nlast", 2, 2, 1, 0, 12, "\n\nselected\n\n", id="blank-lines"
        ),
    ],
)
def test_line_window_returns_exact_offsets_and_text(
    text: str,
    line_start: int,
    preceding: int,
    following: int,
    start: int,
    end: int,
    window: str,
) -> None:
    """Retain exact line boundaries for available and missing context."""
    actual_start, actual_end = _line_window(
        text, line_start, preceding=preceding, following=following
    )

    assert (actual_start, actual_end) == (start, end)
    assert text[actual_start:actual_end] == window


def test_rust_command_flags_are_not_mistaken_for_backend_failures() -> None:
    """Compiler arguments can contain both `error` and `write` as values."""
    log = (
        "DEBUG sccache::server: parse_arguments: Ok: "
        '["--error-format=json", "--cfg", "feature=\\"write\\""]'
    )

    assert sanitize_error_log(log) == (), "ignore compiler-command debug output"
    assert sanitize_error_log("DEBUG compiler command includes 403") == (), (
        "a bare three-digit value is not an HTTP response"
    )


def test_large_debug_logs_have_bounded_diagnostic_cost() -> None:
    """Two hundred thousand debug records stay fast and output-capped."""
    debug_line = (
        "DEBUG sccache::server: parse_arguments: Ok: "
        '["--error-format=json", "--cfg", "feature=\\"write\\"", "status=429"]'
    )
    log = "\n".join((debug_line,) * 200_000)

    start = time.perf_counter()
    lines = sanitize_error_log(log)
    elapsed = time.perf_counter() - start

    assert elapsed < 5, f"sanitizing 200,000 debug records took {elapsed:.3f}s"
    assert len(lines) <= MAX_DIAGNOSTIC_LINES, "cap diagnostic output"


def test_diagnostic_reads_only_the_bounded_log_prefix(
    tmp_path: pathlib.Path, monkeypatch
) -> None:
    """A large server log cannot cause an unbounded diagnostic read."""
    monkeypatch.setattr(report_sccache_errors, "MAX_LOG_BYTES", 10)
    log_file = tmp_path / "sccache-error.log"
    log_file.write_bytes(b"safe prefixERROR failed to write HTTP 429")

    lines = report_sccache_errors._read_diagnostic(log_file, ())

    assert lines == (
        "no backend write failure found in the inspected log prefix (truncated)",
    ), "do not inspect or report log entries beyond the configured byte limit"


def test_empty_error_log_environment_is_treated_as_unavailable(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """An empty optional path reaches the helper's unavailable-file message."""
    monkeypatch.setenv("SCCACHE_ERROR_LOG", "")

    assert report_sccache_errors.parse_arguments([]).log_file is None
