"""Collect operating-system observations for pure test-runner diagnostics."""

from __future__ import annotations

import os
import pathlib
import sys
import time
from typing import Protocol

from test_runner_diagnostics import (
    LockRecord,
    StallReportContext,
    _normalize_device_inode,
    format_stall_report,
    parse_proc_locks,
)
from test_runner_process_io import CommandRequest
from test_runner_process_tree import ProcessTreeSnapshot


class _StallReportTiming(Protocol):
    """Expose supervisor timing values needed for a diagnostic report."""

    started_at: float
    timeout_seconds: float


def emit_stall_diagnostics(
    snapshot: ProcessTreeSnapshot,
    request: CommandRequest,
    timing: _StallReportTiming,
    reason: str,
) -> None:
    """Collect, render, and print one bounded report for a stalled command."""
    try:
        context = _collect_stall_report_context(snapshot, request, timing, reason)
        report = format_stall_report(context)
    except Exception as exc:
        report = f"test runner: {reason}; diagnostics unavailable: {exc}"
    try:
        print(report, file=sys.stderr, flush=True)
    except OSError:
        pass


def _collect_stall_report_context(
    snapshot: ProcessTreeSnapshot,
    request: CommandRequest,
    timing: _StallReportTiming,
    reason: str,
) -> StallReportContext:
    """Capture clock, toolchain, procfs, and lock-file state before rendering."""
    environment = request.environment
    target_directory = (
        pathlib.Path(environment["CARGO_TARGET_DIR"])
        if environment.get("CARGO_TARGET_DIR")
        else None
    )
    elapsed_seconds = time.monotonic() - timing.started_at
    toolchain = _find_toolchain_file(request.cwd)
    toolchain_line = _toolchain_file_line(toolchain) if toolchain is not None else None
    process_observed_at = time.monotonic() if snapshot.live_owned() else 0.0
    lock_records = _read_lock_records() if snapshot.proc_available else None
    identities = (
        tuple(lock_path_identities(environment, target_directory).items())
        if isinstance(lock_records, tuple)
        else ()
    )
    return StallReportContext(
        process_snapshot=snapshot,
        command=tuple(request.command),
        cwd=request.cwd,
        environment=environment,
        elapsed_seconds=elapsed_seconds,
        timeout_seconds=timing.timeout_seconds,
        reason=reason,
        process_observed_at=process_observed_at,
        toolchain_file_line=toolchain_line,
        lock_records=lock_records,
        lock_path_identities=identities,
    )


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


def _read_lock_records() -> tuple[LockRecord, ...] | str:
    """Read and parse the current lock table or return its diagnostic."""
    try:
        lock_text = pathlib.Path("/proc/locks").read_text(encoding="utf-8")
    except OSError as exc:
        return f"  lock diagnostics: could not read /proc/locks: {exc}"
    return parse_proc_locks(lock_text)


def _find_toolchain_file(directory: pathlib.Path) -> pathlib.Path | None:
    """Find the nearest Rust toolchain override file without executing Rust."""
    for candidate_directory in (directory, *directory.parents):
        for name in ("rust-toolchain.toml", "rust-toolchain"):
            candidate = candidate_directory / name
            if candidate.is_file():
                return candidate
    return None


def _toolchain_file_line(toolchain: pathlib.Path) -> str:
    """Read a bounded toolchain-file summary for stall diagnostics."""
    try:
        contents = toolchain.read_text(encoding="utf-8").strip().replace("\n", "; ")
    except OSError:
        contents = "unavailable"
    return f"    toolchain file: {toolchain} ({contents[:512]})"
