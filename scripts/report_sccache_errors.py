#!/usr/bin/env python3
"""Print a bounded, sanitized view of sccache's server error log.

The cache setup action starts sccache with ``SCCACHE_ERROR_LOG`` pointed at a
per-job file. Workflow steps call this script after recording cache health;
the path can be passed explicitly or read from ``SCCACHE_ERROR_LOG``. Only a
small number of sanitized error lines are printed, and an unavailable log
never changes the job result.

Examples
--------
Keep an HTTP status and endpoint host while removing URL credentials and path:

    >>> sanitize_error_log(
    ...     "ERROR failed to write cache: HTTP 429 from "
    ...     "https://user:secret@cache.example/item"
    ... )
    ('ERROR failed to write cache: HTTP 429 from <URL https://cache.example>',)

Choose a per-job log path explicitly when invoking the helper:

    >>> parse_arguments(["/tmp/sccache-error.log"]).log_file.name
    'sccache-error.log'
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from collections.abc import Iterable, Mapping, Sequence
from pathlib import Path
from urllib.parse import urlsplit

MAX_LOG_BYTES = 32 * 1024 * 1024
MAX_DIAGNOSTIC_LINES = 12
MAX_LINE_LENGTH = 1000

_ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
_CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")
_URL = re.compile(r"\b[a-z][a-z0-9+.-]*://[^\s\"'<>]+", re.IGNORECASE)
_BEARER = re.compile(r"\b(Bearer|Basic)\s+[^\s,;]+", re.IGNORECASE)
_SECRET_FIELD = re.compile(
    r"(?i)([\"']?[\w.-]*(?:TOKEN|SECRET|PASSWORD|CREDENTIAL|AUTHORIZATION|"
    r"API[_-]?KEY|ACCESS[_-]?KEY|SIGNATURE|SAS)[\w.-]*[\"']?\s*[:=]\s*)"
    r"(?:\"[^\"]*\"|'[^']*'|[^,\s;}]+)"
)
_GITHUB_TOKEN = re.compile(
    r"\b(?:gh[oprsu]_[A-Za-z0-9_]{8,}|github_pat_[A-Za-z0-9_]{8,})\b"
)
_ERROR = re.compile(
    r"(?<![-\w])(?:error|failed|failure|status|response|caused by|"
    r"http\s+\d{3})(?![-\w])",
    re.IGNORECASE,
)
_WRITE_FAILURE = re.compile(
    r"\b(?:write|store|upload|put)[ \t]+"
    r"(?:request[ \t]+|operation[ \t]+|attempt[ \t]+)?"
    r"(?:failed|failure|error|denied|timeout|rate[- ]limited)\b"
    r"|\b(?:failed|failure|error|denied|timeout|rate[- ]limited)[ \t]+"
    r"(?:to[ \t]+)?(?:write|store|upload|put)\b",
    re.IGNORECASE,
)
_STARTUP_PROBE = re.compile(r"\.sccache_check\b")
# This embeds only the fixed module-level _ERROR pattern, never log input.
_ERROR_LINE = re.compile(rf"(?im)^[^\r\n]*(?:{_ERROR.pattern})[^\r\n]*$")


def _environment_secrets(environment: Mapping[str, str]) -> tuple[str, ...]:
    """Return non-empty values from environment variables named as secrets."""
    secret_name = re.compile(
        r"(?:TOKEN(?:_FILE)?|SECRET|PASSWORD|CREDENTIALS?(?:_FILE)?|"
        r"AUTH(?:_CONFIG|_SOCK)?|"
        r"API[_-]?KEY|ACCESS[_-]?KEY(?:[_-]?ID)?|SIGNATURE|SAS)$",
        re.IGNORECASE,
    )
    return tuple(
        sorted(
            {
                value
                for name, value in environment.items()
                if value and secret_name.search(name)
            },
            key=len,
            reverse=True,
        )
    )


def _redact_url(match: re.Match[str]) -> str:
    """Keep the endpoint host while dropping path and credential-bearing parts."""
    try:
        parts = urlsplit(match.group())
        if parts.hostname:
            return f"<URL {parts.scheme}://{parts.hostname}>"
    except ValueError:
        pass
    return "<URL>"


def _sanitize_line(line: str, secrets: Iterable[str]) -> str:
    """Remove credentials, URLs and control sequences from one log line."""
    safe = _ANSI.sub("", line)
    safe = _CONTROL.sub("?", safe)
    for secret in secrets:
        safe = safe.replace(secret, "<redacted>")
    safe = _URL.sub(_redact_url, safe)
    safe = _BEARER.sub(r"\1 <redacted>", safe)
    safe = _SECRET_FIELD.sub(r"\1<redacted>", safe)
    safe = _GITHUB_TOKEN.sub("<redacted>", safe)
    return safe[:MAX_LINE_LENGTH]


def _line_window(
    text: str, line_start: int, preceding: int, following: int
) -> tuple[int, int]:
    """Return offsets spanning one line and its requested neighbours."""
    start = _preceding_line_start(text, line_start, preceding)
    end = line_start
    for _ in range(following + 1):
        next_newline = text.find("\n", end)
        if next_newline < 0:
            end = len(text)
            break
        end = next_newline + 1
    return start, end


def _preceding_line_start(text: str, line_start: int, preceding: int) -> int:
    """Return the first requested context-line offset before a selected line."""
    start = line_start
    for _ in range(preceding):
        previous_newline = text.rfind("\n", 0, start - 1) if start else -1
        if previous_newline < 0:
            return 0
        start = previous_newline + 1
    return start


def _first_write_failure_start(log_text: str) -> int | None:
    """Find the first non-probe error line describing a backend write failure."""
    for match in _WRITE_FAILURE.finditer(log_text):
        line_start = log_text.rfind("\n", 0, match.start()) + 1
        line_end = log_text.find("\n", match.end())
        line = log_text[line_start : line_end if line_end >= 0 else len(log_text)]
        if _ERROR.search(line) and not _STARTUP_PROBE.search(line):
            return line_start
    return None


def _write_failure_lines(
    log_text: str, start: int = 0, end: int | None = None
) -> list[tuple[int, str]]:
    """Return bounded, non-probe lines that describe backend write failures."""
    selected_end = len(log_text) if end is None else end
    lines: dict[int, str] = {}
    for match in _WRITE_FAILURE.finditer(log_text, start, selected_end):
        line_start = log_text.rfind("\n", 0, match.start()) + 1
        if line_start in lines:
            continue
        line_end = log_text.find("\n", match.end(), selected_end)
        line = log_text[line_start : line_end if line_end >= 0 else selected_end]
        if not _STARTUP_PROBE.search(line):
            lines[line_start] = line
        if len(lines) >= MAX_DIAGNOSTIC_LINES:
            break
    return list(lines.items())


def _error_lines(
    log_text: str, start: int = 0, end: int | None = None
) -> list[tuple[int, str]]:
    """Return bounded error lines in a log range, excluding startup probes."""
    matches = _ERROR_LINE.finditer(
        log_text, start, len(log_text) if end is None else end
    )
    lines = []
    for match in matches:
        if _STARTUP_PROBE.search(match.group()):
            continue
        lines.append((match.start(), match.group()))
        if len(lines) >= MAX_DIAGNOSTIC_LINES:
            break
    return lines


def _nearby_error_lines(
    log_text: str,
    first_write_start: int | None,
    diagnostic_lines: list[tuple[int, str]],
) -> list[tuple[int, str]]:
    """Return diagnostics near the first write failure or reported error."""
    nearby_start, nearby_end = _line_window(
        log_text,
        first_write_start if first_write_start is not None else diagnostic_lines[0][0],
        preceding=2 if first_write_start is not None else 0,
        following=4,
    )
    return _combined_diagnostic_lines(
        _error_lines(log_text, nearby_start, nearby_end),
        _write_failure_lines(log_text, nearby_start, nearby_end),
    )


def _combined_diagnostic_lines(
    *groups: list[tuple[int, str]],
) -> list[tuple[int, str]]:
    """Merge diagnostic groups by source offset without duplicate lines."""
    by_offset = {offset: line for group in groups for offset, line in group}
    return sorted(by_offset.items())


def _prioritize_error_lines(
    nearby: list[tuple[int, str]],
    write_failures: list[tuple[int, str]],
    all_errors: list[tuple[int, str]],
) -> tuple[str, ...]:
    """Prefer backend failures, then nearby context and other error lines."""
    selected: list[str] = []
    selected_offsets: set[int] = set()
    for group in (write_failures, nearby, all_errors):
        for offset, line in group:
            if offset in selected_offsets:
                continue
            selected_offsets.add(offset)
            selected.append(line)
            if len(selected) >= MAX_DIAGNOSTIC_LINES:
                return tuple(selected)
    return tuple(selected)


def sanitize_error_log(
    log_text: str, secret_values: Iterable[str] = ()
) -> tuple[str, ...]:
    """Return bounded sanitized error lines, backend write failures first.

    The sccache error log can include backend request details. This function
    keeps useful status and cause text while removing credential-bearing URLs,
    token fields and known secret values before anything reaches workflow logs.
    """
    secrets = tuple(value for value in secret_values if value)
    first_write_start = _first_write_failure_start(log_text)
    error_lines = _error_lines(log_text)
    write_failure_lines = _write_failure_lines(log_text)
    all_diagnostics = _combined_diagnostic_lines(error_lines, write_failure_lines)
    if not all_diagnostics:
        return ()

    nearby = _nearby_error_lines(log_text, first_write_start, all_diagnostics)
    selected = _prioritize_error_lines(nearby, write_failure_lines, error_lines)
    return tuple(_sanitize_line(line, secrets) for line in selected)


def _read_diagnostic(path: Path, secrets: Iterable[str]) -> tuple[str, ...]:
    """Read only the first bounded portion of the server log."""
    try:
        with path.open("rb") as log_file:
            contents = log_file.read(MAX_LOG_BYTES + 1)
    except OSError:
        return ("sccache error log is unavailable",)

    was_truncated = len(contents) > MAX_LOG_BYTES
    text = contents[:MAX_LOG_BYTES].decode("utf-8", errors="replace")
    lines = sanitize_error_log(text, secrets)
    if lines:
        prefix = (
            "first sccache backend write failure"
            if any(_WRITE_FAILURE.search(line) for line in lines)
            else "sccache error log"
        )
        inspected_bytes = min(len(contents), MAX_LOG_BYTES)
        header = f"{prefix} (inspected {inspected_bytes} bytes"
        if was_truncated:
            header += ", truncated"
        header += ")"
        return (header, *lines)
    return (
        "no backend write failure found in the bounded error log"
        if not was_truncated
        else "no backend write failure found in the inspected log prefix (truncated)",
    )


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse the diagnostic log path."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "log_file",
        nargs="?",
        type=Path,
        default=os.environ.get("SCCACHE_ERROR_LOG") or None,
        help="per-job sccache error log (defaults to SCCACHE_ERROR_LOG)",
    )
    return parser.parse_args(arguments)


def main(arguments: Sequence[str] | None = None) -> int:
    """Print sanitized diagnostics without affecting the failed workflow."""
    options = parse_arguments(arguments)
    if options.log_file is None:
        print("sccache error log path is unavailable")
        return 0
    secrets = _environment_secrets(os.environ)
    for line in _read_diagnostic(options.log_file, secrets):
        # Prefix each line so log content cannot issue a GitHub workflow command.
        print(f"sccache: {line}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
