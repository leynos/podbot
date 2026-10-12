"""Track and clean up only descendants owned by one test-runner process."""

from __future__ import annotations

import dataclasses
import os
import pathlib
import signal
import subprocess
import time
import typing as typ
from collections.abc import Mapping
from types import MappingProxyType

from test_runner_process_snapshot import (
    ProcessInfo,
    _read_process,
    _read_process_table,
    parse_proc_stat as parse_proc_stat,
    snapshot_direct_child_identities as snapshot_direct_child_identities,
)
from test_runner_subreaper import (
    enable_child_subreaper as enable_child_subreaper,
    get_child_subreaper as get_child_subreaper,
    set_child_subreaper as set_child_subreaper,
)
from test_runner_windows_process import signal_windows_process_tree


@dataclasses.dataclass(frozen=True)
class OwnedProcess:
    """A process identity retained across PID reuse."""

    info: ProcessInfo
    first_seen: float


@dataclasses.dataclass(frozen=True)
class ProcessTreeSnapshot:
    """Immutable process state returned by one explicit tree refresh."""

    root_pid: int
    proc_available: bool
    processes: Mapping[int, ProcessInfo]
    owned: tuple[OwnedProcess, ...]

    def __post_init__(self) -> None:
        """Freeze caller-provided containers as well as snapshots from refresh."""
        object.__setattr__(self, "processes", MappingProxyType(dict(self.processes)))
        object.__setattr__(self, "owned", tuple(self.owned))

    def live_owned(self) -> tuple[OwnedProcess, ...]:
        """Return snapshot-owned processes that remain live and identity-matched."""
        if not self.proc_available:
            return ()
        return tuple(
            owned
            for owned in self.owned
            if (current := self.processes.get(owned.info.pid)) is not None
            and current.start_time == owned.info.start_time
            and current.state not in {"Z", "X"}
        )

    def descendants(self) -> tuple[OwnedProcess, ...]:
        """Return live snapshot descendants in the order first tracked."""
        return tuple(
            owned for owned in self.live_owned() if owned.info.pid != self.root_pid
        )

    def ancestors_of(self, pid: int) -> set[int]:
        """Return owned process ancestors using only this captured snapshot."""
        owned_by_pid = {owned.info.pid: owned for owned in self.owned}
        ancestors: set[int] = set()
        current = owned_by_pid.get(pid)
        while current is not None and current.info.parent_pid not in ancestors:
            parent_pid = current.info.parent_pid
            parent = owned_by_pid.get(parent_pid)
            if parent is None:
                break
            ancestors.add(parent_pid)
            current = parent
        return ancestors


@dataclasses.dataclass(frozen=True)
class ProcessTreeRoot:
    """Capture the root identity and adoption boundary for one child tree.

    Construct this only when the supervisor launches a new process group. The
    pre-existing child snapshot prevents subreaper adoption from claiming an
    unrelated process that was already attached to the runner.
    """

    pid: int
    command: str
    started_at: float
    subreaper: bool
    preexisting_child_identities: frozenset[tuple[int, int]] = frozenset()


class OwnedProcessTree:
    """Track a child and its descendants without addressing unrelated PIDs."""

    def __init__(self, root: ProcessTreeRoot) -> None:
        self.root_pid = root.pid
        self.root_command = root.command
        self.started_at = root.started_at
        self.subreaper = root.subreaper
        self.preexisting_child_identities = root.preexisting_child_identities
        self.proc_available = False
        self.owned: dict[int, OwnedProcess] = {}

    def refresh(self) -> ProcessTreeSnapshot:
        """Refresh owned state and return an immutable process snapshot."""
        self.proc_available = pathlib.Path("/proc").is_dir()
        if not self.proc_available:
            return ProcessTreeSnapshot(
                self.root_pid, False, MappingProxyType({}), tuple(self.owned.values())
            )
        processes = _read_process_table()
        root_info = processes.get(self.root_pid)
        if root_info is not None and self.root_pid not in self.owned:
            self.owned[self.root_pid] = OwnedProcess(root_info, self.started_at)
        self._refresh_known_processes(processes)
        self._discover_descendants(processes)
        return ProcessTreeSnapshot(
            self.root_pid,
            True,
            processes,
            tuple(self.owned.values()),
        )

    def _refresh_known_processes(self, processes: dict[int, ProcessInfo]) -> None:
        """Update identities and discard exited or reused descendant PIDs."""
        for pid, owned in tuple(self.owned.items()):
            current = processes.get(pid)
            if current is not None and current.start_time == owned.info.start_time:
                self.owned[pid] = dataclasses.replace(owned, info=current)
            elif pid != self.root_pid:
                del self.owned[pid]

    def _discover_descendants(self, processes: dict[int, ProcessInfo]) -> None:
        """Walk child links from owned roots and newly adopted orphans."""
        children_by_parent: dict[int, list[ProcessInfo]] = {}
        for process in processes.values():
            children_by_parent.setdefault(process.parent_pid, []).append(process)
        pending = list(self.owned)
        self._adopt_new_processes(processes.values(), pending)
        self._walk_owned_children(children_by_parent, pending)

    def _adopt_new_processes(
        self,
        processes: typ.Iterable[ProcessInfo],
        pending: list[int],
    ) -> None:
        """Add only new direct children adopted by this runner as subreaper."""
        for process in processes:
            if self._is_new_adopted_process(process):
                self._record_owned(process)
                pending.append(process.pid)

    def _walk_owned_children(
        self,
        children_by_parent: dict[int, list[ProcessInfo]],
        pending: list[int],
    ) -> None:
        """Walk the descendants reachable from every owned process identity."""
        while pending:
            parent_pid = pending.pop()
            self._record_unowned_children(
                children_by_parent.get(parent_pid, ()), pending
            )

    def _record_unowned_children(
        self,
        children: typ.Iterable[ProcessInfo],
        pending: list[int],
    ) -> None:
        """Record new descendants and queue them for traversal."""
        for process in children:
            if process.pid not in self.owned:
                self._record_owned(process)
                pending.append(process.pid)

    def _is_new_adopted_process(self, process: ProcessInfo) -> bool:
        """Identify a new direct child adopted by this runner as subreaper."""
        identity = (process.pid, process.start_time)
        return (
            self.subreaper
            and process.parent_pid == os.getpid()
            and process.pid != self.root_pid
            and identity not in self.preexisting_child_identities
            and process.pid not in self.owned
        )

    def _record_owned(self, process: ProcessInfo) -> None:
        """Remember one descendant's current PID and start-time identity."""
        self.owned[process.pid] = OwnedProcess(process, time.monotonic())

    def signal_all(self, process: typ.Any, signum: int) -> None:
        """Signal the verified owned tree, falling back to its private group."""
        snapshot = self.refresh()
        if not self.proc_available:
            _signal_process_group(process, self.root_pid, signum)
            return
        self._signal_visible_processes(snapshot, signum)
        self._signal_root(process, signum)

    def _signal_visible_processes(
        self, snapshot: ProcessTreeSnapshot, signum: int
    ) -> None:
        """Signal visible children only while their PID identities still match."""
        for owned in snapshot.owned:
            current = snapshot.processes.get(owned.info.pid)
            if current is not None and current.start_time == owned.info.start_time:
                _signal_identity(current, signum)

    @staticmethod
    def _signal_root(process: typ.Any, signum: int) -> None:
        """Signal the Popen root if it has not already exited."""
        if process.poll() is not None:
            return
        try:
            process.send_signal(signum)
        except ProcessLookupError:
            pass

    def reap_adopted(self) -> None:
        """Reap known adopted grandchildren when running as a subreaper."""
        if not self.subreaper or not hasattr(os, "waitpid"):
            return
        for pid in tuple(self.owned):
            if pid == self.root_pid:
                continue
            try:
                os.waitpid(pid, os.WNOHANG)
            except (ChildProcessError, ProcessLookupError):
                continue

    def terminate(self, process: typ.Any, *, grace_seconds: float = 1.5) -> bool:
        """Terminate, then kill and reap only descendants of the owned child."""
        self.signal_all(process, signal.SIGTERM)
        self._wait_for_tree(process, grace_seconds, repeat_signal=signal.SIGTERM)
        self.signal_all(process, signal.SIGKILL)
        self._wait_for_tree(process, grace_seconds)
        self._wait_for_root(process, grace_seconds)
        self.reap_adopted()
        return process.poll() is not None and not self.refresh().live_owned()

    def _wait_for_tree(
        self,
        process: typ.Any,
        timeout_seconds: float,
        *,
        repeat_signal: int | None = None,
    ) -> None:
        """Wait for root and descendants, repeating a gentle signal if set."""
        deadline = time.monotonic() + timeout_seconds
        while time.monotonic() < deadline:
            self.reap_adopted()
            if process.poll() is not None and not self.refresh().live_owned():
                return
            if repeat_signal is not None:
                self.signal_all(process, repeat_signal)
            time.sleep(0.05)

    @staticmethod
    def _wait_for_root(process: typ.Any, timeout_seconds: float) -> None:
        """Reap the Popen root, escalating if its first wait expires."""
        try:
            process.wait(timeout=timeout_seconds)
        except subprocess.TimeoutExpired:
            if process.poll() is None:
                process.kill()
            try:
                process.wait(timeout=timeout_seconds)
            except subprocess.TimeoutExpired:
                return


def _signal_identity(process: ProcessInfo, signum: int) -> None:
    """Signal a PID only after confirming its procfs start-time identity."""
    current = _read_process(process.pid)
    if current is None or current.start_time != process.start_time:
        return
    if _signal_with_pidfd(process, signum):
        return
    try:
        os.kill(process.pid, signum)
    except OSError:
        pass


def _signal_with_pidfd(process: ProcessInfo, signum: int) -> bool:
    """Use a PID file descriptor when this Python and kernel support it."""
    pidfd_open = getattr(os, "pidfd_open", None)
    pidfd_send_signal = getattr(signal, "pidfd_send_signal", None)
    if pidfd_open is None or pidfd_send_signal is None:
        return False
    try:
        descriptor = pidfd_open(process.pid)
    except OSError:
        return False
    try:
        confirmed = _read_process(process.pid)
        if confirmed is None or confirmed.start_time != process.start_time:
            return True
        pidfd_send_signal(descriptor, signum)
    except OSError:
        return False
    finally:
        os.close(descriptor)
    return True


def _signal_process_group(process: typ.Any, group_id: int, signum: int) -> None:
    """Signal the process group created exclusively for the Cargo child."""
    if os.name != "posix":
        if signal_windows_process_tree(group_id, signum):
            return
        _signal_single_process(process, signum)
        return
    try:
        os.killpg(group_id, signum)
    except ProcessLookupError:
        pass


def _signal_single_process(process: typ.Any, signum: int) -> None:
    """Apply a termination signal where process groups are not available."""
    if process.poll() is not None:
        return
    action = process.terminate if signum == signal.SIGTERM else process.kill
    action()
