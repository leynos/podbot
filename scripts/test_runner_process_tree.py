"""Track and clean up only descendants owned by one test-runner process."""

from __future__ import annotations

import dataclasses
import os
import pathlib
import signal
import sys
import time
import typing as typ


@dataclasses.dataclass(frozen=True)
class ProcessInfo:
    """One process snapshot read from procfs."""

    pid: int
    parent_pid: int
    process_group: int
    start_time: int
    state: str
    command: str


@dataclasses.dataclass(frozen=True)
class OwnedProcess:
    """A process identity retained across PID reuse."""

    info: ProcessInfo
    first_seen: float


def parse_proc_stat(contents: str, command: str = "") -> ProcessInfo:
    """Parse the fields needed from Linux ``/proc/<pid>/stat``.

    The command name is parenthesized and may itself contain spaces or closing
    parentheses, so parsing begins after its final closing parenthesis.
    """
    closing_parenthesis = contents.rfind(")")
    if closing_parenthesis < 0:
        raise ValueError("process stat has no command delimiter")
    pid_text = contents[: contents.find(" ")]
    fields = contents[closing_parenthesis + 1 :].split()
    if len(fields) <= 19:
        raise ValueError("process stat is missing required fields")
    return ProcessInfo(
        pid=int(pid_text),
        parent_pid=int(fields[1]),
        process_group=int(fields[2]),
        start_time=int(fields[19]),
        state=fields[0],
        command=command,
    )


class OwnedProcessTree:
    """Track a child and its descendants without addressing unrelated PIDs."""

    def __init__(
        self,
        root_pid: int,
        root_command: str,
        started_at: float,
        *,
        subreaper: bool,
    ) -> None:
        self.root_pid = root_pid
        self.root_command = root_command
        self.started_at = started_at
        self.subreaper = subreaper
        self.proc_available = pathlib.Path("/proc").is_dir()
        root_info = _read_process(root_pid)
        self.owned: dict[int, OwnedProcess] = {}
        if root_info is not None:
            self.owned[root_pid] = OwnedProcess(root_info, started_at)

    def refresh(self) -> dict[int, ProcessInfo]:
        """Discover descendants and return the current procfs snapshot."""
        if not self.proc_available:
            return {}
        processes = _read_process_table()
        for pid, owned in tuple(self.owned.items()):
            current = processes.get(pid)
            if current is None or current.start_time != owned.info.start_time:
                if pid != self.root_pid:
                    del self.owned[pid]
                continue
            self.owned[pid] = dataclasses.replace(owned, info=current)

        changed = True
        while changed:
            changed = False
            for process in processes.values():
                if process.pid in self.owned:
                    continue
                inherited = process.parent_pid in self.owned
                adopted = (
                    self.subreaper
                    and process.parent_pid == os.getpid()
                    and process.pid != self.root_pid
                )
                if inherited or adopted:
                    self.owned[process.pid] = OwnedProcess(process, time.monotonic())
                    changed = True
        return processes

    def live_owned(self) -> tuple[OwnedProcess, ...]:
        """Return owned processes that have not exited or become zombies."""
        processes = self.refresh()
        if not self.proc_available:
            return ()
        return tuple(
            owned
            for pid, owned in self.owned.items()
            if (current := processes.get(pid)) is not None
            and current.start_time == owned.info.start_time
            and current.state not in {"Z", "X"}
        )

    def descendants(self) -> tuple[OwnedProcess, ...]:
        """Return all currently live tracked processes except the root child."""
        return tuple(
            process
            for process in self.live_owned()
            if process.info.pid != self.root_pid
        )

    def ancestors_of(self, pid: int) -> set[int]:
        """Return owned ancestors of a process from the latest parent links."""
        self.refresh()
        ancestors: set[int] = set()
        current = self.owned.get(pid)
        while current is not None and current.info.parent_pid not in ancestors:
            parent_pid = current.info.parent_pid
            parent = self.owned.get(parent_pid)
            if parent is None:
                break
            ancestors.add(parent_pid)
            current = parent
        return ancestors

    def signal_all(self, process: typ.Any, signum: int) -> None:
        """Signal the verified owned tree, falling back to its private group."""
        if self.proc_available:
            processes = self.refresh()
            for pid, owned in tuple(self.owned.items()):
                current = processes.get(pid)
                if current is None or current.start_time != owned.info.start_time:
                    continue
                _signal_identity(current, signum)
            if process.poll() is None:
                try:
                    process.send_signal(signum)
                except ProcessLookupError:
                    pass
            return
        _signal_process_group(process, self.root_pid, signum)

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
        deadline = time.monotonic() + grace_seconds
        while time.monotonic() < deadline:
            self.reap_adopted()
            if not self.live_owned() and process.poll() is not None:
                break
            self.signal_all(process, signal.SIGTERM)
            time.sleep(0.05)
        self.signal_all(process, signal.SIGKILL)
        kill_deadline = time.monotonic() + grace_seconds
        while time.monotonic() < kill_deadline:
            self.reap_adopted()
            if not self.live_owned() and process.poll() is not None:
                break
            time.sleep(0.05)
        try:
            process.wait(timeout=grace_seconds)
        except TimeoutError:
            if process.poll() is None:
                process.kill()
            try:
                process.wait(timeout=grace_seconds)
            except TimeoutError:
                return False
        self.reap_adopted()
        return process.poll() is not None and not self.live_owned()


def enable_child_subreaper() -> bool:
    """Make this Linux process adopt orphaned descendants when supported."""
    return set_child_subreaper(True)


def get_child_subreaper() -> bool | None:
    """Read the current Linux child-subreaper flag when supported."""
    if not sys_is_linux():
        return None
    try:
        import ctypes

        enabled = ctypes.c_int()
        result = ctypes.CDLL(None, use_errno=True).prctl(
            37, ctypes.byref(enabled), 0, 0, 0
        )
        return bool(enabled.value) if result == 0 else None
    except (AttributeError, OSError):
        return None


def set_child_subreaper(enabled: bool) -> bool:
    """Set Linux child-subreaper behaviour without affecting other platforms."""
    if not sys_is_linux():
        return False
    try:
        import ctypes

        return ctypes.CDLL(None, use_errno=True).prctl(36, int(enabled), 0, 0, 0) == 0
    except (AttributeError, OSError):
        return False


def sys_is_linux() -> bool:
    """Return whether this host provides Linux procfs and prctl semantics."""
    return sys_platform().startswith("linux")


def sys_platform() -> str:
    """Return the interpreter's platform identifier."""
    return sys.platform


def _read_process_table() -> dict[int, ProcessInfo]:
    """Read visible process identities and commands from Linux procfs."""
    table: dict[int, ProcessInfo] = {}
    try:
        process_paths = pathlib.Path("/proc").iterdir()
        for process_path in process_paths:
            if not process_path.name.isdecimal():
                continue
            process = _read_process(int(process_path.name))
            if process is not None:
                table[process.pid] = process
    except OSError:
        return table
    return table


def _read_process(pid: int) -> ProcessInfo | None:
    """Read one process stat and command line when it remains available."""
    try:
        process_path = pathlib.Path("/proc") / str(pid)
        stat = (process_path / "stat").read_text(encoding="utf-8")
        command_line = (process_path / "cmdline").read_bytes()
    except (FileNotFoundError, PermissionError, ProcessLookupError, OSError):
        return None
    command = " ".join(
        part.decode("utf-8", errors="replace")
        for part in command_line.split(b"\0")
        if part
    )
    try:
        return parse_proc_stat(stat, command)
    except ValueError:
        return None


def _signal_identity(process: ProcessInfo, signum: int) -> None:
    """Signal a PID only after confirming its procfs start-time identity."""
    current = _read_process(process.pid)
    if current is None or current.start_time != process.start_time:
        return
    pidfd_open = getattr(os, "pidfd_open", None)
    pidfd_send_signal = getattr(signal, "pidfd_send_signal", None)
    if pidfd_open is not None and pidfd_send_signal is not None:
        try:
            descriptor = pidfd_open(process.pid)
        except OSError:
            descriptor = None
        if descriptor is not None:
            try:
                confirmed = _read_process(process.pid)
                if confirmed is not None and confirmed.start_time == process.start_time:
                    pidfd_send_signal(descriptor, signum)
            except OSError:
                pass
            finally:
                os.close(descriptor)
            return
    try:
        os.kill(process.pid, signum)
    except OSError:
        pass


def _signal_process_group(process: typ.Any, group_id: int, signum: int) -> None:
    """Signal the process group created exclusively for the Cargo child."""
    if os.name == "posix":
        try:
            os.killpg(group_id, signum)
        except ProcessLookupError:
            pass
    elif process.poll() is None:
        if signum == signal.SIGTERM:
            process.terminate()
        else:
            process.kill()
