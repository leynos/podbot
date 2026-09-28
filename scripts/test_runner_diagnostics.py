"""Describe owned subprocesses and Cargo lock waits during stalled tests."""

from __future__ import annotations

import dataclasses
import os
import pathlib
import shlex
import time
from collections.abc import Iterable

from test_runner_process_tree import OwnedProcessTree

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
    configured_cargo_home = environment.get("CARGO_HOME")
    cargo_home = (
        pathlib.Path(configured_cargo_home).expanduser()
        if configured_cargo_home
        else pathlib.Path.home() / ".cargo"
    )
    candidates: dict[str, pathlib.Path] = {
        "Cargo package cache": cargo_home / ".package-cache",
        "Cargo package cache mutation lock": cargo_home / ".package-cache-mutate",
    }
    if target_directory is not None:
        candidates["Cargo target directory"] = target_directory / ".cargo-lock"
        for profile in ("debug", "release"):
            candidates[f"Cargo {profile} target lock"] = (
                target_directory / profile / ".cargo-lock"
            )
    identities: dict[str, str] = {}
    for label, path in candidates.items():
        try:
            stat = path.stat()
        except OSError:
            continue
        major, minor = os.major(stat.st_dev), os.minor(stat.st_dev)
        identities[_normalize_device_inode(f"{major:x}:{minor:x}:{stat.st_ino}")] = (
            f"{label} ({path})"
        )
    return identities


def format_stall_report(
    tree: OwnedProcessTree,
    *,
    command: tuple[str, ...],
    cwd: pathlib.Path,
    environment: dict[str, str],
    target_directory: pathlib.Path | None,
    elapsed_seconds: float,
    timeout_seconds: float,
    reason: str,
) -> str:
    """Render process, toolchain, and known Cargo lock diagnostics."""
    lines = [
        f"test runner: {reason} after {elapsed_seconds:.1f}s "
        f"(deadline {timeout_seconds:.1f}s)",
        f"  command: {shlex.join(command)}",
        f"  working directory: {cwd}",
    ]
    lines.extend(_toolchain_lines(cwd, environment))
    lines.extend(_process_lines(tree))
    lines.extend(_lock_lines(tree, environment, target_directory))
    return "\n".join(lines)


def _toolchain_lines(cwd: pathlib.Path, environment: dict[str, str]) -> list[str]:
    """Describe the selected toolchain and environment used by Cargo."""
    lines = ["  toolchain/environment:"]
    for name in (
        "RUSTUP_TOOLCHAIN",
        "RUSTC",
        "RUSTC_WRAPPER",
        "CARGO",
        "CARGO_HOME",
        "CARGO_TARGET_DIR",
    ):
        if value := environment.get(name):
            lines.append(f"    {name}={value}")
    toolchain = _find_toolchain_file(cwd)
    if toolchain is not None:
        try:
            contents = toolchain.read_text(encoding="utf-8").strip().replace("\n", "; ")
        except OSError:
            contents = "unavailable"
        lines.append(f"    toolchain file: {toolchain} ({contents[:512]})")
    return lines


def _process_lines(tree: OwnedProcessTree) -> list[str]:
    """Describe commands, ancestry, and observed elapsed time for owned PIDs."""
    processes = tree.live_owned()
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
    tree: OwnedProcessTree,
    environment: dict[str, str],
    target_directory: pathlib.Path | None,
) -> list[str]:
    """Map known Cargo lock inodes and classify runner-owned waiters."""
    if not tree.proc_available:
        return ["  lock diagnostics: unsupported; Linux /proc is unavailable"]
    try:
        lock_text = pathlib.Path("/proc/locks").read_text(encoding="utf-8")
    except OSError as exc:
        return [f"  lock diagnostics: could not read /proc/locks: {exc}"]
    identities = lock_path_identities(environment, target_directory)
    locks = parse_proc_locks(lock_text)
    owned_pids = {owned.info.pid for owned in tree.live_owned()}
    relevant = [record for record in locks if record.device_inode in identities]
    lines = ["  known Cargo lock ownership and waiters:"]
    if not relevant:
        lines.append("    no locks on mapped Cargo package-cache or target files")
        return lines
    for record in relevant:
        path = identities[record.device_inode]
        role = "waiter" if record.waiter else "holder"
        owner = "owned" if record.pid in owned_pids else "external"
        lines.append(
            f"    {path}: {role} pid={record.pid} owner={owner} "
            f"{record.lock_type} {record.mode}"
        )
    for waiter in (record for record in relevant if record.waiter):
        if waiter.pid not in owned_pids:
            continue
        holders = tuple(
            record
            for record in relevant
            if not record.waiter and record.device_inode == waiter.device_inode
        )
        classification = classify_lock_waiter(
            holders, owned_pids, tree.ancestors_of(waiter.pid)
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
    if any(holder.pid in owned_ancestors for holder in holders):
        return "parent/descendant lock cycle"
    if any(holder.pid not in owned_pids for holder in holders):
        return "ordinary contention with an external lock holder"
    if not holders:
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
