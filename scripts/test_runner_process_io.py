"""Provide pipe and process-group helpers for the test-runner supervisor."""

from __future__ import annotations

import os
import queue
import subprocess
import threading
import time
import typing as typ

READ_END = object()


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
    output: dict[str, list[str]],
    ended_streams: set[str],
    stdout_handler: typ.Callable[[str], None] | None,
) -> None:
    """Dispatch all output already queued by pipe-reader threads."""
    while True:
        try:
            event = events.get_nowait()
        except queue.Empty:
            return
        dispatch_event(event, output, ended_streams, stdout_handler)


def dispatch_event(
    event: tuple[str, str | object],
    output: dict[str, list[str]],
    ended_streams: set[str],
    stdout_handler: typ.Callable[[str], None] | None,
) -> None:
    """Forward captured chunks and record pipe EOF notifications."""
    stream_name, chunk = event
    if chunk is READ_END:
        ended_streams.add(stream_name)
    elif isinstance(chunk, str):
        if stream_name == "stdout" and stdout_handler is not None:
            stdout_handler(chunk)
        else:
            output[stream_name].append(chunk)


def drain_after_cleanup(
    events: queue.Queue[tuple[str, str | object]],
    output: dict[str, list[str]],
    ended_streams: set[str],
    expected_streams: set[str],
    stdout_handler: typ.Callable[[str], None] | None,
) -> None:
    """Keep pipe readers unblocked after the owned tree is terminated."""
    drain_deadline = time.monotonic() + 2.0
    while ended_streams < expected_streams and time.monotonic() < drain_deadline:
        try:
            event = events.get(timeout=0.05)
        except queue.Empty:
            continue
        dispatch_event(event, output, ended_streams, stdout_handler)


def join_readers(readers: list[threading.Thread]) -> None:
    """Give each drained output reader a bounded opportunity to finish."""
    for reader in readers:
        reader.join(timeout=1.0)


def process_group_arguments() -> dict[str, int | bool]:
    """Create a private process group for descendants launched by this runner."""
    if os.name == "posix":
        return {"start_new_session": True}
    return {"creationflags": getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)}


def normal_exit_status(return_code: int) -> int:
    """Convert subprocess signal exits to the conventional shell status."""
    return return_code if return_code >= 0 else 128 - return_code
