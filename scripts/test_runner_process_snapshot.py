"""Capture explicit process snapshots from Linux procfs."""

from __future__ import annotations

import dataclasses
import os
import pathlib


@dataclasses.dataclass(frozen=True)
class ProcessInfo:
    """One process snapshot read from procfs."""

    pid: int
    parent_pid: int
    process_group: int
    start_time: int
    state: str
    command: str


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


def snapshot_direct_child_identities() -> frozenset[tuple[int, int]]:
    """Capture existing direct children before launching an owned process."""
    if not pathlib.Path("/proc").is_dir():
        return frozenset()
    return frozenset(
        (process.pid, process.start_time)
        for process in _read_process_table().values()
        if process.parent_pid == os.getpid()
    )


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
        stat = (process_path / "stat").read_bytes().decode("utf-8", errors="replace")
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
