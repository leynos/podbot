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
        process_tree.ProcessTreeRoot(
            root_pid,
            "controlled test child",
            0.0,
            True,
            frozenset({(unrelated_pid, 104)}),
        )
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
    monkeypatch.setattr(
        supervisor_module.ProcessSupervisor, "_start_process", fail_to_start
    )
    supervisor = supervisor_module.ProcessSupervisor(1)
    supervisor.subreaper_enabled = True

    status = supervisor._run(
        supervisor_module.CommandRequest(
            ["controlled-child"],
            tmp_path,
            {},
            "ownership ordering test",
        )
    )[0]

    assert status == 127, "a failed spawn must keep the normal launch error status"
    assert order == ["snapshot", "start"], (
        "the host-child baseline must be taken before starting the root process"
    )


def test_pidfd_signal_failure_falls_back_to_identity_checked_kill(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A failed pidfd signal must still attempt the checked PID fallback."""
    process = _process(70005, os.getpid(), 105)
    signalled: list[tuple[int, int]] = []
    closed: list[int] = []

    def fail_pidfd_signal(_descriptor: int, _signum: int) -> None:
        raise OSError("controlled pidfd signal failure")

    monkeypatch.setattr(process_tree, "_read_process", lambda _pid: process)
    monkeypatch.setattr(process_tree.os, "pidfd_open", lambda _pid: 123, raising=False)
    monkeypatch.setattr(
        process_tree.signal,
        "pidfd_send_signal",
        fail_pidfd_signal,
        raising=False,
    )
    monkeypatch.setattr(process_tree.os, "close", closed.append)
    monkeypatch.setattr(
        process_tree.os,
        "kill",
        lambda pid, sig: signalled.append((pid, sig)),
    )

    process_tree._signal_identity(process, 15)

    assert signalled == [(process.pid, 15)], (
        "failed pidfd signalling must retry after checking process identity"
    )
    assert closed == [123], "the pidfd must close after a failed signal"


def test_pidfd_identity_mismatch_suppresses_pid_fallback(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A reused PID must not be signalled through the numeric-PID fallback."""
    process = _process(70006, os.getpid(), 106)
    reused = _process(process.pid, os.getpid(), 999)
    sent: list[tuple[int, int]] = []
    killed: list[tuple[int, int]] = []
    identities = iter((process, reused))

    monkeypatch.setattr(process_tree, "_read_process", lambda _pid: next(identities))
    monkeypatch.setattr(process_tree.os, "pidfd_open", lambda _pid: 124, raising=False)
    monkeypatch.setattr(
        process_tree.signal,
        "pidfd_send_signal",
        lambda descriptor, signum: sent.append((descriptor, signum)),
        raising=False,
    )
    monkeypatch.setattr(process_tree.os, "close", lambda _descriptor: None)
    monkeypatch.setattr(
        process_tree.os,
        "kill",
        lambda pid, sig: killed.append((pid, sig)),
    )

    process_tree._signal_identity(process, 15)

    assert sent == [], "PID reuse must prevent signalling the stale process"
    assert killed == [], "PID reuse must suppress the numeric-PID fallback"


@pytest.mark.skipif(sys.platform != "linux", reason="procfs tracking is Linux-only")
def test_read_process_replaces_invalid_utf8_in_stat_command_name(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Opaque procfs command bytes must not abort process-tree inspection."""
    pid = 70007
    stat_path = pathlib.Path("/proc") / str(pid) / "stat"
    command_path = pathlib.Path("/proc") / str(pid) / "cmdline"
    stat = (
        f"{pid} (worker ".encode()
        + b"\xff) S 1 "
        + b" ".join(str(value).encode() for value in range(2, 23))
    )
    real_read_bytes = pathlib.Path.read_bytes

    def read_bytes(path: pathlib.Path) -> bytes:
        if path == stat_path:
            return stat
        if path == command_path:
            return b"/usr/bin/worker\0argument\0"
        return real_read_bytes(path)

    monkeypatch.setattr(pathlib.Path, "read_bytes", read_bytes)

    process = process_tree._read_process(pid)

    assert process is not None, (
        "invalid UTF-8 in the process command name must not discard its record"
    )
    assert process.parent_pid == 1, "the stat parent PID must remain parseable"
    assert process.command == "/usr/bin/worker argument", (
        "the process command line must retain its replacement-safe decoding"
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
