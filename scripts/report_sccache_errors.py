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
    r"\b(?:error|failed|failure|status|response|caused by|http\s+\d{3}|[1-5]\d{2})\b",
    re.IGNORECASE,
)
_WRITE = re.compile(
    r"\b(?:write|writes|writing|written|store|storing|stored|upload|uploaded|put|persist)\b",
    re.IGNORECASE,
)
_STARTUP_PROBE = re.compile(r"\.sccache_check\b")


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


def sanitize_error_log(
    log_text: str, secret_values: Iterable[str] = ()
) -> tuple[str, ...]:
    """Return a bounded set of sanitized error lines, write-related first.

    The sccache error log can include backend request details. This function
    keeps useful status and cause text while removing credential-bearing URLs,
    token fields and known secret values before anything reaches workflow logs.
    """
    secrets = tuple(value for value in secret_values if value)
    lines = log_text.splitlines()
    error_indices = [
        index
        for index, line in enumerate(lines)
        if _ERROR.search(line) and not _STARTUP_PROBE.search(line)
    ]
    write_indices = [index for index in error_indices if _WRITE.search(lines[index])]
    if not error_indices:
        return ()

    first_write = write_indices[0] if write_indices else error_indices[0]
    nearby = [
        index
        for index in error_indices
        if max(0, first_write - 2) <= index <= first_write + 4
    ]
    selected = nearby or error_indices
    selected.extend(index for index in error_indices if index not in selected)
    return tuple(
        _sanitize_line(lines[index], secrets)
        for index in selected[:MAX_DIAGNOSTIC_LINES]
    )


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
            "first write-related sccache error"
            if any(_WRITE.search(line) for line in lines)
            else "sccache error log"
        )
        inspected_bytes = min(len(contents), MAX_LOG_BYTES)
        header = f"{prefix} (inspected {inspected_bytes} bytes"
        if was_truncated:
            header += ", truncated"
        header += ")"
        return (header, *lines)
    return (
        "no write-related sccache error found in the bounded error log"
        if not was_truncated
        else "no write-related error found in the inspected log prefix (truncated)",
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
