"""Provide pipe and process-group helpers for the test-runner supervisor."""

from __future__ import annotations

import os
import pathlib
import queue
import subprocess
import threading
import time
import typing as typ
from dataclasses import dataclass, field

READ_END = object()


@dataclass(frozen=True)
class ProcessLaunchRequest:
    """Group the process-I/O values needed for one child launch."""

    command: list[str]
    cwd: pathlib.Path
    environment: dict[str, str]
    stdout_pipe: bool
    stderr_pipe: bool
    suspended: bool = False


@dataclass
class StreamCapture:
    """Collect one command's selected output streams until they reach EOF."""

    expected_streams: set[str]
    stdout_handler: typ.Callable[[str], None] | None = None
    output: dict[str, list[str]] = field(
        default_factory=lambda: {"stdout": [], "stderr": []}
    )
    ended_streams: set[str] = field(default_factory=set)


def piped_stream_names(stdout_pipe: bool, stderr_pipe: bool) -> tuple[str, ...]:
    """Return the standard streams that need reader threads."""
    return tuple(
        name
        for name, enabled in (("stdout", stdout_pipe), ("stderr", stderr_pipe))
        if enabled
    )


def read_stream(
    stream_name: str,
    stream: typ.TextIO,
    events: queue.Queue[tuple[str, str | object]],
) -> None:
    """Move child output to the supervising thread without blocking its timer."""
    try:
        for line in stream:
            events.put((stream_name, line))
    finally:
        events.put((stream_name, READ_END))


def drain_events(
    events: queue.Queue[tuple[str, str | object]],
    capture: StreamCapture,
) -> None:
    """Dispatch all output already queued by pipe-reader threads."""
    while True:
        try:
            event = events.get_nowait()
        except queue.Empty:
            return
        dispatch_event(event, capture)


def dispatch_event(
    event: tuple[str, str | object],
    capture: StreamCapture,
) -> None:
    """Forward captured chunks and record pipe EOF notifications."""
    stream_name, chunk = event
    if chunk is READ_END:
        capture.ended_streams.add(stream_name)
    elif isinstance(chunk, str):
        if stream_name == "stdout" and capture.stdout_handler is not None:
            capture.stdout_handler(chunk)
        else:
            capture.output[stream_name].append(chunk)


def drain_after_cleanup(
    events: queue.Queue[tuple[str, str | object]],
    capture: StreamCapture,
) -> None:
    """Keep pipe readers unblocked after the owned tree is terminated."""
    drain_deadline = time.monotonic() + 2.0
    while (
        capture.ended_streams < capture.expected_streams
        and time.monotonic() < drain_deadline
    ):
        try:
            event = events.get(timeout=0.05)
        except queue.Empty:
            continue
        dispatch_event(event, capture)


def join_readers(readers: list[threading.Thread]) -> None:
    """Give each drained output reader a bounded opportunity to finish."""
    for reader in readers:
        reader.join(timeout=1.0)


def process_group_arguments(*, suspended: bool = False) -> dict[str, int | bool]:
    """Create a private process group and optionally suspend its first process."""
    if os.name == "posix":
        return {"start_new_session": True}
    flags = getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
    if suspended:
        flags |= getattr(subprocess, "CREATE_SUSPENDED", 0)
    return {"creationflags": flags}


def start_process(request: ProcessLaunchRequest) -> subprocess.Popen[str]:
    """Launch one command in a private process group with selected pipes."""
    return subprocess.Popen(
        request.command,
        cwd=request.cwd,
        env=request.environment,
        stdin=None,
        stdout=subprocess.PIPE if request.stdout_pipe else None,
        stderr=subprocess.PIPE if request.stderr_pipe else None,
        text=True,
        encoding="utf-8",
        errors="replace",
        close_fds=True,
        **process_group_arguments(suspended=request.suspended),
    )


def start_readers(
    process: subprocess.Popen[str],
    stdout_pipe: bool,
    stderr_pipe: bool,
    stdout_handler: typ.Callable[[str], None] | None,
) -> tuple[
    queue.Queue[tuple[str, str | object]], list[threading.Thread], StreamCapture
]:
    """Read piped child streams concurrently so monitoring remains bounded."""
    events: queue.Queue[tuple[str, str | object]] = queue.Queue()
    readers: list[threading.Thread] = []
    expected_streams = set(piped_stream_names(stdout_pipe, stderr_pipe))
    for stream_name in expected_streams:
        stream = process.stdout if stream_name == "stdout" else process.stderr
        if stream is None:
            continue
        reader = threading.Thread(
            target=read_stream,
            args=(stream_name, stream, events),
            name=f"test-runner-{stream_name}-{process.pid}",
            daemon=True,
        )
        reader.start()
        readers.append(reader)
    return events, readers, StreamCapture(expected_streams, stdout_handler)


def normal_exit_status(return_code: int) -> int:
    """Convert subprocess signal exits to the conventional shell status."""
    return return_code if return_code >= 0 else 128 - return_code
