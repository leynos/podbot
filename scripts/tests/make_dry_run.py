"""Dry-run a Make gate target, for the contracts that read what it would run."""

from __future__ import annotations

import subprocess
from pathlib import Path


def dry_run_recipe(
    environment: dict[str, str],
    overrides: tuple[str, ...] = (),
    target: str = "test",
) -> str:
    """Return normalized output from a dry-run of a Make gate target."""
    repository_root = Path(__file__).resolve().parents[2]
    output = subprocess.run(
        ["make", "--no-print-directory", "--dry-run", target, *overrides],
        cwd=repository_root,
        env=environment,
        capture_output=True,
        check=True,
        text=True,
    ).stdout
    return " ".join(output.replace("\\\n\t", " ").split())
