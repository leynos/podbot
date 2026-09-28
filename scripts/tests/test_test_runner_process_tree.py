"""Check that subreaper tracking excludes children predating a test run."""

from __future__ import annotations

import os
import pathlib
import sys

import pytest

import test_runner_process_tree as process_tree
import test_runner_supervisor as supervisor_module


@pytest.mark.skipif(sys.platform != "linux", reason="procfs tracking is Linux-only")
def test_subreaper_adopts_owned_orphan_but_not_preexisting_child(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Orphaned owned descendants remain cleanable without claiming neighbours."""
    runner_pid = os.getpid()
    root_pid = 70001
    child_pid = 70002
    grandchild_pid = 70003
    unrelated_pid = 70004
    identities = {
        root_pid: _process(root_pid, runner_pid, 101),
        child_pid: _process(child_pid, root_pid, 102),
        grandchild_pid: _process(grandchild_pid, child_pid, 103),
        unrelated_pid: _process(unrelated_pid, runner_pid, 104),
    }
    current = dict(identities)
    monkeypatch.setattr(process_tree, "_read_process", lambda pid: identities.get(pid))
    monkeypatch.setattr(process_tree, "_read_process_table", lambda: dict(current))

    tree = process_tree.OwnedProcessTree(
        root_pid,
        "controlled test child",
        0.0,
        subreaper=True,
        preexisting_child_identities=frozenset({(unrelated_pid, 104)}),
    )

    tree.refresh()
    assert unrelated_pid not in tree.owned, (
        "a direct child present before the run must not become runner-owned"
    )
    assert grandchild_pid in tree.owned, "owned descendants must remain tracked"

    current.pop(child_pid)
    current[grandchild_pid] = _process(grandchild_pid, runner_pid, 103)
    tree.refresh()

    assert grandchild_pid in tree.owned, (
        "a newly adopted orphan from the owned tree must remain cleanable"
    )
    assert unrelated_pid not in tree.owned, (
        "refreshing adopted orphans must continue to exclude preexisting children"
    )


@pytest.mark.skipif(sys.platform != "linux", reason="procfs tracking is Linux-only")
def test_subreaper_snapshot_precedes_runner_child_launch(
    monkeypatch: pytest.MonkeyPatch, tmp_path: pathlib.Path
) -> None:
    """Ownership baseline is captured before the supervised child can start."""
    order: list[str] = []

    def snapshot() -> frozenset[tuple[int, int]]:
        order.append("snapshot")
        return frozenset()

    def fail_to_start(*_args: object) -> None:
        order.append("start")
        raise OSError("controlled spawn failure")

    monkeypatch.setattr(supervisor_module, "snapshot_direct_child_identities", snapshot)
    monkeypatch.setattr(supervisor_module.ProcessSupervisor, "_start_process", fail_to_start)
    supervisor = supervisor_module.ProcessSupervisor(1)
    supervisor.subreaper_enabled = True

    status = supervisor._run(
        ["controlled-child"],
        tmp_path,
        {},
        purpose="ownership ordering test",
    )[0]

    assert status == 127, "a failed spawn must keep the normal launch error status"
    assert order == ["snapshot", "start"], (
        "the host-child baseline must be taken before starting the root process"
    )


def _process(pid: int, parent_pid: int, start_time: int) -> process_tree.ProcessInfo:
    """Build a stable fake process identity for procfs ownership tests."""
    return process_tree.ProcessInfo(
        pid=pid,
        parent_pid=parent_pid,
        process_group=pid,
        start_time=start_time,
        state="S",
        command=f"process-{pid}",
    )
