"""Select pure terminal-state predicates for supervised child processes."""

from __future__ import annotations

import signal

from test_runner_process_tree import ProcessTreeSnapshot
from test_runner_windows_process import WindowsProcessJob


def signal_terminal_state(signum: int | None) -> tuple[int, str] | None:
    """Return the shell status and reason for a received signal."""
    if signum is None:
        return None
    return 128 + signum, f"interrupted by {signal.Signals(signum).name}"


def has_leaked_descendants(
    snapshot: ProcessTreeSnapshot,
    return_status: int | None,
    windows_job: WindowsProcessJob | None,
) -> bool:
    """Check captured descendants and Windows job membership after exit."""
    if return_status is None:
        return False
    if snapshot.descendants():
        return True
    return bool(windows_job is not None and (windows_job.active_processes() or 0) > 0)
