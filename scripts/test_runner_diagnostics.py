"""Describe owned subprocesses and Cargo lock waits during stalled tests."""

from __future__ import annotations

import dataclasses
import os
import pathlib
import shlex
import sys
import time
from collections.abc import Iterable
from typing import Protocol

from test_runner_process_io import CommandRequest
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
    target_directory: pathlib.Path | None
    elapsed_seconds: float
    timeout_seconds: float
    reason: str


class _StallReportTiming(Protocol):
    """Expose the supervisor timing values needed by a stall report."""

    started_at: float
    timeout_seconds: float


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


def lock_path_identities(
    environment: dict[str, str], target_directory: pathlib.Path | None
) -> dict[str, str]:
    """Map known Cargo lock-file device/inode pairs to readable paths."""
    candidates = _known_lock_candidates(environment, target_directory)
    return {
        identity: f"{label} ({path})"
        for label, path in candidates.items()
        if (identity := _path_identity(path)) is not None
    }


def _known_lock_candidates(
    environment: dict[str, str], target_directory: pathlib.Path | None
) -> dict[str, pathlib.Path]:
    """Collect package-cache and selected target-directory lock paths."""
    cargo_home = _cargo_home(environment)
    candidates = _cargo_home_lock_candidates(cargo_home)
    return {
        **candidates,
        **(
            _target_lock_candidates(target_directory)
            if target_directory is not None
            else {}
        ),
    }


def _cargo_home(environment: dict[str, str]) -> pathlib.Path:
    """Resolve Cargo's configured home without changing the process environment."""
    configured = environment.get("CARGO_HOME")
    return (
        pathlib.Path(configured).expanduser()
        if configured
        else pathlib.Path.home() / ".cargo"
    )


def _path_identity(path: pathlib.Path) -> str | None:
    """Return a stable device/inode key for a visible Cargo lock file."""
    try:
        stat = path.stat()
    except OSError:
        return None
    major, minor = os.major(stat.st_dev), os.minor(stat.st_dev)
    return _normalize_device_inode(f"{major:x}:{minor:x}:{stat.st_ino}")


def _cargo_home_lock_candidates(cargo_home: pathlib.Path) -> dict[str, pathlib.Path]:
    """Map Cargo-home package-cache locks to their configured paths."""
    return {
        "Cargo package cache": cargo_home / ".package-cache",
        "Cargo package cache mutation lock": cargo_home / ".package-cache-mutate",
    }


def _target_lock_candidates(
    target_directory: pathlib.Path,
) -> dict[str, pathlib.Path]:
    """Map Cargo target-directory locks for common build profiles."""
    return {
        label: target_directory / relative
        for label, relative in (
            ("Cargo target directory", pathlib.Path(".cargo-lock")),
            ("Cargo debug target lock", pathlib.Path("debug/.cargo-lock")),
            ("Cargo release target lock", pathlib.Path("release/.cargo-lock")),
        )
    }


def format_stall_report(context: StallReportContext) -> str:
    """Render process, toolchain, and known Cargo lock diagnostics."""
    lines = [
        f"test runner: {context.reason} after {context.elapsed_seconds:.1f}s "
        f"(deadline {context.timeout_seconds:.1f}s)",
        f"  command: {shlex.join(context.command)}",
        f"  working directory: {context.cwd}",
    ]
    lines.extend(_toolchain_lines(context.cwd, context.environment))
    lines.extend(_process_lines(context.process_snapshot))
    lines.extend(
        _lock_lines(
            context.process_snapshot, context.environment, context.target_directory
        )
    )
    return "\n".join(lines)


def emit_stall_diagnostics(
    snapshot: ProcessTreeSnapshot,
    request: CommandRequest,
    timing: _StallReportTiming,
    reason: str,
) -> None:
    """Build and print one bounded report for a supervised command."""
    try:
        context = StallReportContext(
            process_snapshot=snapshot,
            command=tuple(request.command),
            cwd=request.cwd,
            environment=request.environment,
            target_directory=(
                pathlib.Path(request.environment["CARGO_TARGET_DIR"])
                if request.environment.get("CARGO_TARGET_DIR")
                else None
            ),
            elapsed_seconds=time.monotonic() - timing.started_at,
            timeout_seconds=timing.timeout_seconds,
            reason=reason,
        )
        report = format_stall_report(context)
    except Exception as exc:
        report = f"test runner: {reason}; diagnostics unavailable: {exc}"
    try:
        print(report, file=sys.stderr, flush=True)
    except OSError:
        pass


def _toolchain_lines(cwd: pathlib.Path, environment: dict[str, str]) -> list[str]:
    """Describe the selected toolchain and environment used by Cargo."""
    return [
        "  toolchain/environment:",
        *_configured_toolchain_lines(environment),
        *_toolchain_file_lines(cwd),
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


def _toolchain_file_lines(cwd: pathlib.Path) -> list[str]:
    """Summarize the nearest repository toolchain file when present."""
    toolchain = _find_toolchain_file(cwd)
    return [_toolchain_file_line(toolchain)] if toolchain is not None else []


def _toolchain_file_line(toolchain: pathlib.Path) -> str:
    """Read a bounded toolchain-file summary for stall diagnostics."""
    try:
        contents = toolchain.read_text(encoding="utf-8").strip().replace("\n", "; ")
    except OSError:
        contents = "unavailable"
    return f"    toolchain file: {toolchain} ({contents[:512]})"


def _process_lines(snapshot: ProcessTreeSnapshot) -> list[str]:
    """Describe commands, ancestry, and observed elapsed time for owned PIDs."""
    processes = snapshot.live_owned()
    lines = ["  owned process tree:"]
    if processes:
        now = time.monotonic()
        for owned in sorted(processes, key=lambda item: item.info.pid):
            info = owned.info
            lines.append(
                f"    pid={info.pid} ppid={info.parent_pid} "
                f"elapsed={max(0.0, now - owned.first_seen):.1f}s "
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
    environment: dict[str, str],
    target_directory: pathlib.Path | None,
) -> list[str]:
    """Map known Cargo lock inodes and classify runner-owned waiters."""
    if not snapshot.proc_available:
        return ["  lock diagnostics: unsupported; Linux /proc is unavailable"]
    records_or_error = _read_lock_records()
    if isinstance(records_or_error, str):
        return [records_or_error]
    return _format_lock_records(
        snapshot, records_or_error, environment, target_directory
    )


def _read_lock_records() -> tuple[LockRecord, ...] | str:
    """Read and parse the current lock table or return its diagnostic."""
    try:
        lock_text = pathlib.Path("/proc/locks").read_text(encoding="utf-8")
    except OSError as exc:
        return f"  lock diagnostics: could not read /proc/locks: {exc}"
    return parse_proc_locks(lock_text)


def _format_lock_records(
    snapshot: ProcessTreeSnapshot,
    records: tuple[LockRecord, ...],
    environment: dict[str, str],
    target_directory: pathlib.Path | None,
) -> list[str]:
    """Map parsed lock records to known paths and classify owned waiters."""
    identities = lock_path_identities(environment, target_directory)
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


def _find_toolchain_file(directory: pathlib.Path) -> pathlib.Path | None:
    """Find the nearest Rust toolchain override file without executing Rust."""
    for candidate_directory in (directory, *directory.parents):
        for name in ("rust-toolchain.toml", "rust-toolchain"):
            candidate = candidate_directory / name
            if candidate.is_file():
                return candidate
    return None


def _normalize_device_inode(identity: str) -> str:
    """Ignore procfs zero-padding when matching device and inode identities."""
    major, minor, inode = identity.split(":", maxsplit=2)
    return f"{int(major, 16):x}:{int(minor, 16):x}:{int(inode, 10)}"
