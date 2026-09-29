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
_ERROR_LINE = re.compile(rf"(?im)^[^\r\n]*(?:{_ERROR.pattern})[^\r\n]*$")


def _environment_secrets(environment: Mapping[str, str]) -> tuple[str, ...]:
    """Return non-empty values from environment variables named as secrets."""
    secret_name = re.compile(
        r"(?:TOKEN|SECRET|PASSWORD|CREDENTIAL|AUTH|API[_-]?KEY|ACCESS[_-]?KEY)$",
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
    safe = _URL.sub(_redact_url, safe)
    safe = _BEARER.sub(r"\1 <redacted>", safe)
    safe = _SECRET_FIELD.sub(r"\1<redacted>", safe)
    safe = _GITHUB_TOKEN.sub("<redacted>", safe)
    for secret in secrets:
        safe = safe.replace(secret, "<redacted>")
    return safe[:MAX_LINE_LENGTH]


def _line_window(
    text: str, line_start: int, preceding: int, following: int
) -> tuple[int, int]:
    """Return offsets spanning one line and its requested neighbours."""
    start = line_start
    for _ in range(preceding):
        previous_newline = text.rfind("\n", 0, start)
        if previous_newline < 0:
            start = 0
            break
        start = previous_newline

    end = line_start
    for _ in range(following + 1):
        next_newline = text.find("\n", end)
        if next_newline < 0:
            end = len(text)
            break
        end = next_newline + 1
    return start, end


def sanitize_error_log(
    log_text: str, secret_values: Iterable[str] = ()
) -> tuple[str, ...]:
    """Return bounded sanitized error lines, backend write failures first.

    The sccache error log can include backend request details. This function
    keeps useful status and cause text while removing credential-bearing URLs,
    token fields and known secret values before anything reaches workflow logs.
    """
    secrets = tuple(value for value in secret_values if value)
    first_write_start = None
    for match in _WRITE_FAILURE.finditer(log_text):
        line_start = log_text.rfind("\n", 0, match.start()) + 1
        line_end = log_text.find("\n", match.end())
        line = log_text[line_start : line_end if line_end >= 0 else len(log_text)]
        if not _STARTUP_PROBE.search(line):
            first_write_start = line_start
            break

    error_lines = []
    for match in _ERROR_LINE.finditer(log_text):
        if _STARTUP_PROBE.search(match.group()):
            continue
        error_lines.append((match.start(), match.group()))
        if len(error_lines) >= MAX_DIAGNOSTIC_LINES:
            break

    if not error_lines:
        return ()

    nearby_start, nearby_end = _line_window(
        log_text,
        first_write_start if first_write_start is not None else error_lines[0][0],
        preceding=2 if first_write_start is not None else 0,
        following=4,
    )
    nearby = [
        (match.start(), match.group())
        for match in _ERROR_LINE.finditer(log_text, nearby_start, nearby_end)
        if not _STARTUP_PROBE.search(match.group())
    ]

    selected: list[str] = []
    selected_offsets: set[int] = set()
    for index_group in (nearby, error_lines):
        for offset, line in index_group:
            if offset in selected_offsets:
                continue
            selected_offsets.add(offset)
            selected.append(line)
            if len(selected) >= MAX_DIAGNOSTIC_LINES:
                break
        if len(selected) >= MAX_DIAGNOSTIC_LINES:
            break
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
