#!/usr/bin/env python3
"""Tell later steps whether the CodeScene token is available, without passing it on.

The upload action is composite, and a composite action hands the calling
step's `env` to every step nested inside it, including artefact and cache
steps that have no use for the secret. So the token is bound only on the
step that runs this script. The script writes `available=true` or
`available=false` to the step's outputs, the upload step's `if:` reads
that output, and the action receives the token through its `access-token`
input alone.

It also records the upload decision for maintainers, without the secret: a
`::notice::` line in the log and a line in the job summary, each naming
whether the token is available, the ref, and whether the upload will run.
Both are bounded to fixed values, so no token-derived text can reach them.

The script is stdlib-only and runs on the runner image's own `python3`.

Example
-------
    CS_ACCESS_TOKEN=... python3 scripts/codescene_token_available.py
"""

from __future__ import annotations

import os
import sys
from collections.abc import Mapping
from pathlib import Path

#: The secret whose presence is reported, never its value.
TOKEN_VARIABLE = "CS_ACCESS_TOKEN"

#: The files GitHub reads step outputs and the job summary from.
OUTPUT_VARIABLE = "GITHUB_OUTPUT"
SUMMARY_VARIABLE = "GITHUB_STEP_SUMMARY"

#: The only ref the upload runs from.
MAIN_REF = "refs/heads/main"


def is_available(environment: Mapping[str, str]) -> bool:
    """Return whether the token is set to something other than whitespace.

    Parameters
    ----------
    environment : Mapping[str, str]
        The step's environment.

    Returns
    -------
    bool
        True when the token is present and not blank.

    Examples
    --------
    >>> is_available({"CS_ACCESS_TOKEN": "secret"})
    True
    >>> is_available({"CS_ACCESS_TOKEN": "  "})
    False
    """
    return bool(environment.get(TOKEN_VARIABLE, "").strip())


def availability(environment: Mapping[str, str]) -> str:
    """Return the step-output line saying whether the token is available.

    Parameters
    ----------
    environment : Mapping[str, str]
        The step's environment.

    Returns
    -------
    str
        Exactly `available=true` or `available=false`.

    Examples
    --------
    >>> availability({"CS_ACCESS_TOKEN": "secret"})
    'available=true'
    >>> availability({})
    'available=false'
    """
    return f"available={'true' if is_available(environment) else 'false'}"


def decision(environment: Mapping[str, str]) -> str:
    """Return the non-secret record of what the upload step will do.

    Parameters
    ----------
    environment : Mapping[str, str]
        The step's environment, including `GITHUB_REF`.

    Returns
    -------
    str
        A fixed-vocabulary record: the operation, whether the token is
        available, whether the ref is main, and the decision.

    Examples
    --------
    >>> decision({"CS_ACCESS_TOKEN": "x", "GITHUB_REF": "refs/heads/main"})
    'operation=codescene_upload token_available=true ref=main decision=upload'
    >>> decision({"GITHUB_REF": "refs/heads/main"})
    'operation=codescene_upload token_available=false ref=main decision=skip_no_token'
    """
    available = is_available(environment)
    on_main = environment.get("GITHUB_REF", "") == MAIN_REF
    if not on_main:
        outcome = "skip_not_main"
    elif not available:
        outcome = "skip_no_token"
    else:
        outcome = "upload"
    return (
        f"operation=codescene_upload token_available={str(available).lower()} "
        f"ref={'main' if on_main else 'other'} decision={outcome}"
    )


def _append(path: str, line: str) -> None:
    """Append one line to a file GitHub reads after the step."""
    with Path(path).open("a", encoding="utf-8") as handle:
        handle.write(line + "\n")


def main(environment: Mapping[str, str] | None = None) -> int:
    """Write the availability output and record the decision.

    The environment is injected so tests can drive this without touching the
    process environment. A missing `GITHUB_OUTPUT` is an error rather than a
    silent success: without it the upload's guard would read an empty output
    and skip on every run, with nothing failing.

    Parameters
    ----------
    environment : Mapping[str, str] or None
        The step's environment; the process environment when omitted.

    Returns
    -------
    int
        0 on success, 1 when the output file is not configured.
    """
    environment = os.environ if environment is None else environment
    output = environment.get(OUTPUT_VARIABLE, "")
    if not output:
        print(f"::error::{OUTPUT_VARIABLE} is not set, so no output can be written")
        return 1
    _append(output, availability(environment))
    record = decision(environment)
    print(f"::notice::{record}")
    summary = environment.get(SUMMARY_VARIABLE, "")
    if summary:
        _append(summary, f"CodeScene upload: `{record}`")
    return 0


if __name__ == "__main__":
    sys.exit(main())
