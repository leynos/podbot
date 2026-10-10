"""Describe owned subprocesses and Cargo lock waits during stalled tests."""

from __future__ import annotations

import dataclasses
import pathlib
import shlex
from collections.abc import Iterable

from test_runner_process_tree import ProcessTreeSnapshot

_PROCESS_COMMAND_LIMIT = 256


@dataclasses.dataclass(frozen=True)
class LockRecord:
    """One lock holder or waiter reported by Linux procfs."""

    lock_id: str
    waiter: bool
    lock_type: str
    mode: str
    pid: int
    device_inode: str


@dataclasses.dataclass(frozen=True)
class StallReportContext:
    """Capture one process state for the runner's bounded-failure report.

    The supervisor is the only producer. Keep the report inputs together so
    diagnostics reflect one command and its environment at the same instant.
    """

    process_snapshot: ProcessTreeSnapshot
    command: tuple[str, ...]
    cwd: pathlib.Path
    environment: dict[str, str]
    elapsed_seconds: float
    timeout_seconds: float
    reason: str
    process_observed_at: float
    toolchain_file_line: str | None
    lock_records: tuple[LockRecord, ...] | str | None
    lock_path_identities: tuple[tuple[str, str], ...]


def parse_proc_locks(contents: str) -> tuple[LockRecord, ...]:
    """Parse holder and waiter records from ``/proc/locks`` text."""
    records: list[LockRecord] = []
    for line in contents.splitlines():
        fields = line.split()
        if len(fields) < 7:
            continue
        lock_id = fields[0].rstrip(":")
        offset = 1
        waiter = fields[offset] == "->"
        if waiter:
            offset += 1
        if len(fields) < offset + 6:
            continue
        try:
            pid = int(fields[offset + 3])
            device_inode = _normalize_device_inode(fields[offset + 4])
        except ValueError:
            continue
        records.append(
            LockRecord(
                lock_id=lock_id,
                waiter=waiter,
                lock_type=fields[offset],
                mode=fields[offset + 2],
                pid=pid,
                device_inode=device_inode,
            )
        )
    return tuple(records)


def format_stall_report(context: StallReportContext) -> str:
    """Render process, toolchain, and known Cargo lock observations."""
    lines = [
        f"test runner: {context.reason} after {context.elapsed_seconds:.1f}s "
        f"(deadline {context.timeout_seconds:.1f}s)",
        f"  command: {shlex.join(context.command)}",
        f"  working directory: {context.cwd}",
    ]
    lines.extend(_toolchain_lines(context.environment, context.toolchain_file_line))
    lines.extend(_process_lines(context.process_snapshot, context.process_observed_at))
    lines.extend(
        _lock_lines(
            context.process_snapshot,
            context.lock_records,
            context.lock_path_identities,
        )
    )
    return "\n".join(lines)


def _toolchain_lines(
    environment: dict[str, str], toolchain_file_line: str | None
) -> list[str]:
    """Describe supplied toolchain details and Cargo's selected environment."""
    return [
        "  toolchain/environment:",
        *_configured_toolchain_lines(environment),
        *([toolchain_file_line] if toolchain_file_line is not None else []),
    ]


def _configured_toolchain_lines(environment: dict[str, str]) -> list[str]:
    """List the environment values that can affect Cargo or rustc."""
    names = (
        "RUSTUP_TOOLCHAIN",
        "RUSTC",
        "RUSTC_WRAPPER",
        "CARGO",
        "CARGO_HOME",
        "CARGO_TARGET_DIR",
    )
    return [f"    {name}={value}" for name in names if (value := environment.get(name))]


def _process_lines(snapshot: ProcessTreeSnapshot, observed_at: float) -> list[str]:
    """Describe commands, ancestry, and captured elapsed time for owned PIDs."""
    processes = snapshot.live_owned()
    lines = ["  owned process tree:"]
    if processes:
        for owned in sorted(processes, key=lambda item: item.info.pid):
            info = owned.info
            lines.append(
                f"    pid={info.pid} ppid={info.parent_pid} "
                f"elapsed={max(0.0, observed_at - owned.first_seen):.1f}s "
                f"state={info.state} "
                f"command={_display_process_command(info.command)}"
            )
    else:
        lines.append("    no live owned process entries are visible")
    return lines


def _display_process_command(command: str) -> str:
    """Keep diagnostic command lines readable while retaining their prefix."""
    if len(command) <= _PROCESS_COMMAND_LIMIT:
        return command or "[unavailable]"
    visible_length = _PROCESS_COMMAND_LIMIT - len("... [truncated]")
    return f"{command[:visible_length]}... [truncated]"


def _lock_lines(
    snapshot: ProcessTreeSnapshot,
    records_or_error: tuple[LockRecord, ...] | str | None,
    identities: tuple[tuple[str, str], ...],
) -> list[str]:
    """Render collected lock records and classify runner-owned waiters."""
    if not snapshot.proc_available:
        return ["  lock diagnostics: unsupported; Linux /proc is unavailable"]
    if isinstance(records_or_error, str):
        return [records_or_error]
    if records_or_error is None:
        return ["  lock diagnostics: unsupported; Linux /proc is unavailable"]
    return _format_lock_records(snapshot, records_or_error, dict(identities))


def _format_lock_records(
    snapshot: ProcessTreeSnapshot,
    records: tuple[LockRecord, ...],
    identities: dict[str, str],
) -> list[str]:
    """Map parsed lock records to known paths and classify owned waiters."""
    owned_pids = {owned.info.pid for owned in snapshot.live_owned()}
    relevant = tuple(record for record in records if record.device_inode in identities)
    if not relevant:
        return [
            "  known Cargo lock ownership and waiters:",
            "    no locks on mapped Cargo package-cache or target files",
        ]
    return [
        "  known Cargo lock ownership and waiters:",
        *_visible_lock_lines(relevant, identities, owned_pids),
        *_waiter_diagnosis_lines(snapshot, relevant, identities, owned_pids),
    ]


def _visible_lock_lines(
    records: tuple[LockRecord, ...], identities: dict[str, str], owned_pids: set[int]
) -> list[str]:
    """Describe visible holders and waiters with their lock ownership."""
    return [_format_lock_record(record, identities, owned_pids) for record in records]


def _format_lock_record(
    record: LockRecord, identities: dict[str, str], owned_pids: set[int]
) -> str:
    """Format one visible Cargo-related lock record."""
    path = identities[record.device_inode]
    role = "waiter" if record.waiter else "holder"
    owner = "owned" if record.pid in owned_pids else "external"
    return (
        f"    {path}: {role} pid={record.pid} owner={owner} "
        f"{record.lock_type} {record.mode}"
    )


def _waiter_diagnosis_lines(
    snapshot: ProcessTreeSnapshot,
    records: tuple[LockRecord, ...],
    identities: dict[str, str],
    owned_pids: set[int],
) -> list[str]:
    """Classify each lock request made by a live owned process."""
    lines: list[str] = []
    for waiter in records:
        if not waiter.waiter or waiter.pid not in owned_pids:
            continue
        holders = tuple(
            record
            for record in records
            if not record.waiter and record.device_inode == waiter.device_inode
        )
        classification = classify_lock_waiter(
            holders, owned_pids, snapshot.ancestors_of(waiter.pid)
        )
        lines.append(
            f"    diagnosis: {classification} "
            f"({identities.get(waiter.device_inode, waiter.device_inode)})"
        )
    return lines


def classify_lock_waiter(
    holders: Iterable[LockRecord],
    owned_pids: set[int],
    owned_ancestors: set[int],
) -> str:
    """Distinguish runner-owned parent cycles from ordinary lock contention."""
    holder_records = tuple(holders)
    if any(holder.pid in owned_ancestors for holder in holder_records):
        return "parent/descendant lock cycle"
    if any(holder.pid not in owned_pids for holder in holder_records):
        return "ordinary contention with an external lock holder"
    if not holder_records:
        return "waiter has no visible holder for this lock"
    return "contention between owned sibling processes"


def _normalize_device_inode(identity: str) -> str:
    """Ignore procfs zero-padding when matching device and inode identities."""
    major, minor, inode = identity.split(":", maxsplit=2)
    return f"{int(major, 16):x}:{int(minor, 16):x}:{int(inode, 10)}"
