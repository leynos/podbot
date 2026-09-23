#!/usr/bin/env python3
"""Tell later steps whether the CodeScene token is available, without passing it on.

The publisher's upload step used to bind `CS_ACCESS_TOKEN` in its own `env`
and test `env.CS_ACCESS_TOKEN != ''`. The upload action is composite, and a
composite action hands the calling step's `env` to every step nested inside
it, including the artefact and cache steps that have no use for the secret.
The token check therefore moves to a step of its own: this script runs there
with the token in its environment and writes only `available=true` or
`available=false` to the step's outputs. The upload step's `if:` reads that
output, and the action receives the token through its `access-token` input
alone.

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

#: The file GitHub reads step outputs from.
OUTPUT_VARIABLE = "GITHUB_OUTPUT"


def availability(environment: Mapping[str, str]) -> str:
    """Return the output line saying whether the token is set and non-blank.

    Example
    -------
        >>> availability({"CS_ACCESS_TOKEN": "secret"})
        'available=true'
        >>> availability({"CS_ACCESS_TOKEN": "  "})
        'available=false'
        >>> availability({})
        'available=false'
    """
    present = bool(environment.get(TOKEN_VARIABLE, "").strip())
    return f"available={'true' if present else 'false'}"


def main(environment: Mapping[str, str] | None = None) -> int:
    """Append the availability line to the step's output file.

    The environment is injected so the tests can drive it without touching
    the process environment. A missing `GITHUB_OUTPUT` is an error rather
    than a silent success: without it the upload's guard would read an empty
    output and skip on every run, with nothing failing.
    """
    environment = os.environ if environment is None else environment
    output = environment.get(OUTPUT_VARIABLE, "")
    if not output:
        print(f"::error::{OUTPUT_VARIABLE} is not set, so no output can be written")
        return 1
    with Path(output).open("a", encoding="utf-8") as handle:
        handle.write(availability(environment) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
