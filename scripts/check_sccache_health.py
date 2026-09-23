#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Fail a lane whose compiler cache is structurally broken, and warn on noise.

The coverage lanes write sccache's statistics as JSON after the coverage
step; this reads that file and decides whether the lane's cache integration is
working. Ported unchanged in substance from leynos/whitaker, where the rule
was measured. A green build says nothing about that: sccache falls back to local
disk, or fails every store, and still compiles.

Three conditions fail the job, because each is a broken integration rather
than a bad day:

- the cache location is not the expected backend, so the server bound
  something other than the store the lane was configured for;
- sccache handled no compile requests, so it wrapped nothing;
- every store attempt failed, or every read did, which is the signature of an
  endpoint the server cannot use (Whitaker's runs 33748602187 and 33756048103
  failed 3,788 of 3,788 stores against GitHub's v2 service).

Isolated read errors, write errors, timeouts and cache errors are reported as
warnings. A proxy hiccup costs one compile, and turning it into a red pull
request would reintroduce the external-service failure that moving CodeScene
off the pull-request lane removed.

The script is stdlib-only and runs on the runner image's own `python3`; it
also declares itself a uv script so it can run the other way.

Example
-------
    python3 scripts/check_sccache_health.py --expect-location ghac sccache-stats.json
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import sys
from collections.abc import Mapping, Sequence
from pathlib import Path

DEFAULT_STATISTICS = Path("sccache-stats.json")

#: Counters whose nonzero value is worth a warning on its own.
WARNED_COUNTERS: tuple[str, ...] = (
    "cache_read_errors",
    "cache_write_errors",
    "cache_timeouts",
)


@dataclasses.dataclass(frozen=True)
class Assessment:
    """What one lane's statistics say: failures first, then warnings."""

    failures: tuple[str, ...]
    warnings: tuple[str, ...]


def _counted(value: object) -> int:
    """Return a counter as an integer, summing a per-language breakdown.

    sccache reports some counters as plain integers and others as
    ``{"counts": {"Rust": n, ...}}``; both mean a total.

    Example
    -------
        >>> _counted({"counts": {"Rust": 3, "C/C++": 2}})
        5
    """
    if isinstance(value, int):
        return value
    if isinstance(value, Mapping):
        counts = value.get("counts", {})
        if isinstance(counts, Mapping):
            return sum(_counted(item) for item in counts.values())
    return 0


def _structural_failures(
    stats: Mapping[str, object], location: str, expected: str
) -> list[str]:
    """Return the conditions that mean the integration is broken."""
    writes = _counted(stats.get("cache_writes"))
    write_errors = _counted(stats.get("cache_write_errors"))
    hits = _counted(stats.get("cache_hits"))
    read_errors = _counted(stats.get("cache_read_errors"))
    checks = (
        (
            not location.startswith(expected),
            f"cache location is {location!r}, not the {expected!r} backend",
        ),
        (
            _counted(stats.get("compile_requests")) == 0,
            "sccache handled no compile requests, so it wrapped nothing",
        ),
        (
            write_errors > 0 and writes == 0,
            f"every store failed ({write_errors} write errors, no writes)",
        ),
        (
            read_errors > 0 and hits == 0,
            f"every read failed ({read_errors} read errors, no hits)",
        ),
    )
    return [message for failed, message in checks if failed]


def assess(document: Mapping[str, object], expected: str) -> Assessment:
    """Judge one `sccache --show-stats --stats-format json` document.

    Parameters
    ----------
    document : Mapping[str, object]
        The parsed statistics.
    expected : str
        The prefix the cache location must start with, such as ``ghac``.

    Returns
    -------
    Assessment
        The structural failures, and a warning for each nonzero error counter.

    Example
    -------
        >>> assess({"cache_location": "ghac, name: x", "stats": {
        ...     "compile_requests": 1, "cache_writes": 1}}, "ghac").failures
        ()
    """
    stats = document.get("stats", {})
    stats = stats if isinstance(stats, Mapping) else {}
    location = str(document.get("cache_location", ""))
    failures = _structural_failures(stats, location, expected)
    counters = (*WARNED_COUNTERS, "cache_errors")
    warnings = [
        f"{name} is {count}"
        for name in counters
        if (count := _counted(stats.get(name))) > 0
    ]
    return Assessment(tuple(failures), tuple(warnings))


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse the command line."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--expect-location",
        required=True,
        help="prefix the reported cache location must start with, such as ghac",
    )
    parser.add_argument("statistics", nargs="?", type=Path, default=DEFAULT_STATISTICS)
    return parser.parse_args(arguments)


def main(arguments: Sequence[str] | None = None) -> int:
    """Report the assessment as workflow commands and return the exit status."""
    options = parse_arguments(arguments)
    try:
        document = json.loads(options.statistics.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"::error::cannot read {options.statistics}: {error}")
        return 1
    if not isinstance(document, Mapping):
        print(f"::error::{options.statistics} is not a statistics object")
        return 1
    assessment = assess(document, options.expect_location)
    for warning in assessment.warnings:
        print(f"::warning::sccache {warning}")
    for failure in assessment.failures:
        print(f"::error::sccache {failure}")
    return 1 if assessment.failures else 0


if __name__ == "__main__":
    sys.exit(main())
