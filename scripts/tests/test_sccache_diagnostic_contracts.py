"""The cache server starts with diagnostics enabled and reports safely."""

from __future__ import annotations

import dataclasses
import pathlib
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
READERS_ACTION = "./.github/actions/sccache-readers"


@dataclasses.dataclass(frozen=True)
class LineWindowCase:
    """Keep input and exact expected offsets together for one line window."""

    text: str
    line_start: int
    preceding: int
    following: int
    start: int
    end: int
    window: str


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
    }, f"{workflow}: diagnostic log must use this job's runner temporary directory"
    assert str(configure.get("run", "")).splitlines() == [
        "printf 'SCCACHE_LOG=debug\\n' >> \"$GITHUB_ENV\"",
        'printf \'SCCACHE_ERROR_LOG=%s\\n\' "$SCCACHE_ERROR_LOG" >> "$GITHUB_ENV"',
    ], f"{workflow}: both diagnostic settings must persist for server restarts"
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
    health_action_index = next(
        index for index, step in enumerate(steps) if step.get("uses") == READERS_ACTION
    )
    diagnostic = steps[diagnostic_index]
    assert diagnostic_index > health_action_index, (
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


def test_environment_secret_names_cover_platform_credentials() -> None:
    """Known AWS, SSH, auth-config, and token-file values are redacted."""
    environment = {
        "AWS_ACCESS_KEY_ID": "aws-access-key",
        "AWS_SESSION_TOKEN_FILE": "session-token.example",
        "SSH_AUTH_SOCK": "/tmp/agent.sock",
        "CACHE_AUTH_CONFIG": "cache.internal",
    }
    secrets = report_sccache_errors._environment_secrets(environment)

    assert set(secrets) == set(environment.values()), (
        "credential-bearing suffixes must contribute their secret values"
    )
    line = sanitize_error_log(
        "ERROR failed to write from https://cache.internal/path "
        "https://session-token.example/item "
        "AWS_ACCESS_KEY_ID=aws-access-key SSH_AUTH_SOCK=/tmp/agent.sock",
        secrets,
    )[0]
    assert "cache.internal" not in line, "redact known secrets before URL handling"
    assert "session-token.example" not in line, (
        "redact token-file values before URL handling"
    )
    assert "aws-access-key" not in line, "redact access-key identifiers"
    assert "/tmp/agent.sock" not in line, "redact SSH agent socket paths"


@pytest.mark.parametrize(
    "line",
    ["WARN write denied", "WARN upload timeout", "WARN put rate-limited"],
    ids=["denied", "timeout", "rate-limited"],
)
def test_write_failure_without_error_keyword_is_reported(line: str) -> None:
    """Write-failure categories remain visible without a generic error token."""
    assert report_sccache_errors._first_write_failure_start(line) is None, (
        "context priority requires a line that also matches the error pattern"
    )
    startup_probe = ".sccache_check: ERROR failed to write cache"
    assert sanitize_error_log(f"{startup_probe}\n{line}") == (line,), (
        "supported write failures must not be discarded without error keywords"
    )


def test_late_write_failure_is_prioritized_over_earlier_generic_errors() -> None:
    """The bounded report retains a write failure after many unrelated errors."""
    earlier_errors = "\n".join(
        f"ERROR unrelated failure {index}" for index in range(10)
    )
    write_failure = "WARN upload timeout"

    lines = sanitize_error_log(f"{earlier_errors}\n{write_failure}")

    assert write_failure in lines, "a later backend failure must outrank generic errors"


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
    "case",
    [
        pytest.param(
            LineWindowCase("first\nselected\nlast\n", 6, 0, 0, 6, 15, "selected\n"),
            id="zero-context",
        ),
        pytest.param(
            LineWindowCase("selected\nmiddle\nlast", 0, 2, 0, 0, 9, "selected\n"),
            id="at-start",
        ),
        pytest.param(
            LineWindowCase("first\nselected", 6, 0, 1, 6, 14, "selected"),
            id="at-end",
        ),
        pytest.param(
            LineWindowCase("one\nselected\n", 4, 9, 9, 0, 13, "one\nselected\n"),
            id="excess-context",
        ),
        pytest.param(
            LineWindowCase("before\nlast", 7, 1, 0, 0, 11, "before\nlast"),
            id="no-final-newline",
        ),
        pytest.param(LineWindowCase("", 0, 0, 0, 0, 0, ""), id="empty-text"),
        pytest.param(
            LineWindowCase("\n\nselected\n\nlast", 2, 2, 1, 0, 12, "\n\nselected\n\n"),
            id="blank-lines",
        ),
        pytest.param(
            LineWindowCase("first\n\n\nselected\n", 8, 1, 0, 7, 17, "\nselected\n"),
            id="consecutive-blank-boundary",
        ),
    ],
)
def test_line_window_returns_exact_offsets_and_text(
    case: LineWindowCase,
) -> None:
    """Retain exact line boundaries for available and missing context."""
    actual_start, actual_end = _line_window(
        case.text,
        case.line_start,
        preceding=case.preceding,
        following=case.following,
    )

    assert (actual_start, actual_end) == (case.start, case.end), (
        "the selected context must retain its exact source offsets"
    )
    assert case.text[actual_start:actual_end] == case.window, (
        "the selected context must retain the exact expected text"
    )


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


def test_large_debug_logs_have_bounded_diagnostic_output() -> None:
    """Large debug logs never expand the diagnostic beyond its output cap."""
    debug_line = (
        "DEBUG sccache::server: parse_arguments: Ok: "
        '["--error-format=json", "--cfg", "feature=\\"write\\"", "status=429"]'
    )
    log = "\n".join((debug_line,) * 200_000)

    lines = sanitize_error_log(log)

    assert len(lines) <= MAX_DIAGNOSTIC_LINES, "cap diagnostic output"


def test_diagnostic_reads_only_the_bounded_log_prefix(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
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

    assert report_sccache_errors.parse_arguments([]).log_file is None, (
        "an empty optional log path must be treated as unavailable"
    )
