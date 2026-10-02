"""Start a configured sccache server outside supervised child ownership."""

from __future__ import annotations

import pathlib
import shlex
import subprocess


def configured_sccache_start_command(
    environment: dict[str, str],
) -> tuple[str, ...] | None:
    """Return a best-effort start command for a configured sccache wrapper."""
    wrapper = tuple(shlex.split(environment.get("RUSTC_WRAPPER", "")))
    if not wrapper or pathlib.Path(wrapper[0]).name.lower() not in {
        "sccache",
        "sccache.exe",
    }:
        return None
    return (*wrapper, "--start-server")


def start_configured_sccache(environment: dict[str, str]) -> None:
    """Start sccache before process supervision, without making it required."""
    command = configured_sccache_start_command(environment)
    if command is None:
        return
    try:
        subprocess.run(
            command,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return
