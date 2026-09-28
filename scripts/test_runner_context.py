"""Carry one test-runner invocation's process resources between phases."""

from __future__ import annotations

import dataclasses
import pathlib

from test_runner_supervisor import ProcessSupervisor


@dataclasses.dataclass(frozen=True)
class TestRunnerContext:
    """Share process inputs between orchestration and Cargo helpers.

    The context belongs to one `scripts/test_runner.py` invocation. It carries
    the resolved Cargo command, working directory, inherited environment, and
    supervisor; the Cargo test plan remains the owner of target selection.
    """

    __test__ = False

    cargo_command: tuple[str, ...]
    cwd: pathlib.Path
    environment: dict[str, str]
    supervisor: ProcessSupervisor
