"""Provide bounded supervision for one-off diagnostic commands.

Use `uv run --no-project --python 3.14 python scripts/test_runner.py
--supervise --timeout 180 -- cargo test` to run a legacy Cargo command with
process-tree and lock diagnostics.
"""

from __future__ import annotations

import os
import pathlib
import shlex

from test_runner_models import RunnerError
from test_runner_supervisor import ProcessSupervisor


def supervise_command(
    command: list[str],
    timeout_seconds: float,
    watch_interval_seconds: float,
    *,
    enable_subreaper: bool,
) -> int:
    """Run one arbitrary command with the test runner's cleanup policy."""
    if not command:
        raise RunnerError("supervise mode requires a command after `--`")
    working_directory = pathlib.Path.cwd().resolve()
    environment = os.environ.copy()
    print(
        f"== Supervised command (timeout {timeout_seconds:g}s): "
        f"{shlex.join(command)} ==",
        flush=True,
    )
    with ProcessSupervisor(
        timeout_seconds,
        watch_interval_seconds,
        enable_subreaper=enable_subreaper,
    ) as supervisor:
        return supervisor.run_inherited(
            command,
            working_directory,
            environment,
            purpose="one-off supervised command",
        )
