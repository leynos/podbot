"""Specify terminal-outcome precedence for the process supervisor."""

from __future__ import annotations

import signal
import time
import types

import pytest

from test_runner_process_tree import OwnedProcess, ProcessInfo, ProcessTreeSnapshot
from test_runner_supervisor import ProcessSupervisor


def test_received_signal_precedes_timeout_and_leaked_descendants() -> None:
    """An interrupt keeps priority over simultaneous timeout and child leaks."""
    supervisor = ProcessSupervisor(timeout_seconds=1)
    supervisor.deadline = time.monotonic() - 1
    supervisor._received_signal = signal.SIGTERM
    snapshot = _snapshot_with_descendant()
    windows_job = types.SimpleNamespace(active_processes=lambda: 1)

    reason = supervisor._terminal_reason(
        snapshot, types.SimpleNamespace(), 0, windows_job
    )

    assert reason == "interrupted by SIGTERM", "signal handling must retain priority"
    assert supervisor.terminal_status == 128 + signal.SIGTERM, (
        "signal status must remain the selected terminal status"
    )


def test_timeout_precedes_leaked_descendants() -> None:
    """A deadline keeps priority over cleanup reporting for leaked children."""
    supervisor = ProcessSupervisor(timeout_seconds=1)
    supervisor.deadline = time.monotonic() - 1

    reason = supervisor._terminal_reason(
        _snapshot_with_descendant(), types.SimpleNamespace(), 0
    )

    assert reason == "timed out", "timeout must precede descendant cleanup"
    assert supervisor.terminal_status == 124, "timeout must keep its exit status"


@pytest.mark.parametrize("descendant_source", ["snapshot", "windows-job"])
def test_exited_command_with_owned_descendants_requires_cleanup(
    descendant_source: str,
) -> None:
    """Captured or Windows-tracked descendants select the cleanup outcome."""
    supervisor = ProcessSupervisor(timeout_seconds=30)
    snapshot = (
        _snapshot_with_descendant()
        if descendant_source == "snapshot"
        else ProcessTreeSnapshot(1, False, {}, ())
    )
    windows_job = (
        types.SimpleNamespace(active_processes=lambda: 2)
        if descendant_source == "windows-job"
        else None
    )

    reason = supervisor._terminal_reason(
        snapshot, types.SimpleNamespace(), 0, windows_job
    )

    assert reason == "command exited while owned descendants remained", (
        f"{descendant_source} descendants must require cleanup"
    )
    assert supervisor.terminal_status == 125, "leaked children must keep cleanup status"


def test_running_command_without_descendants_has_no_terminal_reason() -> None:
    """An active root with no descendant or deadline remains supervised."""
    supervisor = ProcessSupervisor(timeout_seconds=30)
    snapshot = ProcessTreeSnapshot(1, False, {}, ())

    reason = supervisor._terminal_reason(snapshot, types.SimpleNamespace(), None)

    assert reason is None, "an active root must not select a terminal outcome"
    assert supervisor.terminal_status is None, "the supervisor must remain active"


def _snapshot_with_descendant() -> ProcessTreeSnapshot:
    """Create one live owned child for terminal-precedence tests."""
    child = ProcessInfo(
        pid=2,
        parent_pid=1,
        process_group=1,
        start_time=22,
        state="S",
        command="rustc",
    )
    return ProcessTreeSnapshot(
        root_pid=1,
        proc_available=True,
        processes={child.pid: child},
        owned=(OwnedProcess(child, time.monotonic()),),
    )
