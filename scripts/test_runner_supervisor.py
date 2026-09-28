"""Run Cargo and test processes under one deadline and owned-tree policy."""

from __future__ import annotations

import os
import pathlib
import queue
import shlex
import signal
import subprocess
import sys
import threading
import time
import typing as typ

from test_runner_diagnostics import format_stall_report
from test_runner_process_tree import (
    OwnedProcessTree,
    enable_child_subreaper,
    get_child_subreaper,
    set_child_subreaper,
)

_TIMEOUT_EXIT = 124
_CLEANUP_EXIT = 125
_READ_END = object()


class ProcessSupervisor:
    """Supervise subprocesses started by a single test-runner invocation."""

    def __init__(
        self,
        timeout_seconds: float,
        watch_interval_seconds: float = 30.0,
        *,
        enable_subreaper: bool = False,
    ) -> None:
        if timeout_seconds <= 0:
            raise ValueError("test-runner timeout must be greater than zero")
        if watch_interval_seconds <= 0:
            raise ValueError("test-runner watch interval must be greater than zero")
        self.timeout_seconds = timeout_seconds
        self.watch_interval_seconds = watch_interval_seconds
        self.started_at = time.monotonic()
        self.deadline = self.started_at + timeout_seconds
        self.enable_subreaper = enable_subreaper
        self.subreaper_enabled = False
        self.terminal_status: int | None = None
        self._received_signal: int | None = None
        self._saved_handlers: dict[int, typ.Any] = {}
        self._active_tree: OwnedProcessTree | None = None
        self._previous_subreaper: bool | None = None

    def __enter__(self) -> ProcessSupervisor:
        """Install temporary signal handling and enable Linux orphan adoption."""
        if threading.current_thread() is threading.main_thread():
            for signum in (signal.SIGINT, signal.SIGTERM):
                self._saved_handlers[signum] = signal.getsignal(signum)
                signal.signal(signum, self._record_signal)
        if self.enable_subreaper:
            self._previous_subreaper = get_child_subreaper()
            self.subreaper_enabled = enable_child_subreaper()
            if not self.subreaper_enabled and sys.platform != "win32":
                print(
                    "test runner: Linux child-subreaper support is unavailable; "
                    "process-group cleanup remains enabled",
                    file=sys.stderr,
                    flush=True,
                )
        return self

    def __exit__(self, *_: object) -> None:
        """Restore the caller's signal handlers after all children are reaped."""
        for signum, handler in self._saved_handlers.items():
            signal.signal(signum, handler)
        if self._previous_subreaper is not None:
            set_child_subreaper(self._previous_subreaper)

    def run_inherited(
        self,
        command: list[str],
        cwd: pathlib.Path,
        environment: dict[str, str],
        *,
        purpose: str,
    ) -> int:
        """Run one command with inherited standard streams."""
        status, _, _ = self._run(command, cwd, environment, purpose=purpose)
        return status

    def run_capture(
        self,
        command: list[str],
        cwd: pathlib.Path,
        environment: dict[str, str],
        *,
        purpose: str,
    ) -> tuple[int, str, str]:
        """Capture standard output and error while continuing to supervise."""
        return self._run(
            command,
            cwd,
            environment,
            purpose=purpose,
            capture_stdout=True,
            capture_stderr=True,
        )

    def run_lines(
        self,
        command: list[str],
        cwd: pathlib.Path,
        environment: dict[str, str],
        *,
        purpose: str,
        on_line: typ.Callable[[str], None],
    ) -> int:
        """Stream one command's standard output through a line callback."""
        status, _, _ = self._run(
            command,
            cwd,
            environment,
            purpose=purpose,
            stdout_handler=on_line,
        )
        return status

    def _run(
        self,
        command: list[str],
        cwd: pathlib.Path,
        environment: dict[str, str],
        *,
        purpose: str,
        capture_stdout: bool = False,
        capture_stderr: bool = False,
        stdout_handler: typ.Callable[[str], None] | None = None,
    ) -> tuple[int, str, str]:
        """Launch, monitor, diagnose and reap one supervised command."""
        if self.terminal_status is not None:
            return self.terminal_status, "", ""
        stdout_pipe = capture_stdout or stdout_handler is not None
        stderr_pipe = capture_stderr
        started_at = time.monotonic()
        try:
            process = subprocess.Popen(
                command,
                cwd=cwd,
                env=environment,
                stdin=None,
                stdout=subprocess.PIPE if stdout_pipe else None,
                stderr=subprocess.PIPE if stderr_pipe else None,
                text=True,
                encoding="utf-8",
                errors="replace",
                close_fds=True,
                **_process_group_arguments(),
            )
        except OSError as exc:
            print(f"test runner: could not start {command[0]}: {exc}", file=sys.stderr)
            return 127, "", ""

        tree = OwnedProcessTree(
            process.pid,
            shlex.join(command),
            started_at,
            subreaper=self.subreaper_enabled,
        )
        self._active_tree = tree
        events: queue.Queue[tuple[str, str | object]] = queue.Queue(maxsize=256)
        readers: list[threading.Thread] = []
        for stream_name in _piped_stream_names(stdout_pipe, stderr_pipe):
            stream = process.stdout if stream_name == "stdout" else process.stderr
            if stream is not None:
                reader = threading.Thread(
                    target=_read_stream,
                    args=(stream_name, stream, events),
                    name=f"test-runner-{stream_name}-{process.pid}",
                    daemon=True,
                )
                reader.start()
                readers.append(reader)

        output: dict[str, list[str]] = {"stdout": [], "stderr": []}
        ended_streams: set[str] = set()
        expected_streams = {
            name for name in _piped_stream_names(stdout_pipe, stderr_pipe)
        }
        next_watch = time.monotonic() + self.watch_interval_seconds
        exit_status: int | None = None
        while True:
            self._drain_events(events, output, ended_streams, stdout_handler)
            tree.refresh()
            tree.reap_adopted()
            return_status = process.poll()

            if self.terminal_status is None and self._received_signal is not None:
                signum = self._received_signal
                self.terminal_status = 128 + signum
                self._emit_diagnostics(
                    tree,
                    command,
                    cwd,
                    environment,
                    started_at,
                    f"interrupted by {signal.Signals(signum).name}",
                )
                self._terminate_tree(tree, process)
            elif self.terminal_status is None and time.monotonic() >= self.deadline:
                self.terminal_status = _TIMEOUT_EXIT
                self._emit_diagnostics(
                    tree, command, cwd, environment, started_at, "timed out"
                )
                self._terminate_tree(tree, process)
            elif (
                self.terminal_status is None
                and return_status is not None
                and tree.descendants()
            ):
                self.terminal_status = _CLEANUP_EXIT
                self._emit_diagnostics(
                    tree,
                    command,
                    cwd,
                    environment,
                    started_at,
                    "command exited while owned descendants remained",
                )
                self._terminate_tree(tree, process)

            if self.terminal_status is not None:
                self._drain_after_cleanup(
                    events, output, ended_streams, expected_streams, stdout_handler
                )
                exit_status = self.terminal_status
                break

            if return_status is not None and ended_streams >= expected_streams:
                exit_status = _normal_exit_status(return_status)
                break

            now = time.monotonic()
            if now >= next_watch:
                self._emit_diagnostics(
                    tree,
                    command,
                    cwd,
                    environment,
                    started_at,
                    f"still running ({purpose})",
                )
                next_watch = now + self.watch_interval_seconds
            wait_seconds = min(0.1, max(0.0, self.deadline - now))
            wait_seconds = min(wait_seconds, max(0.0, next_watch - now))
            try:
                event = events.get(timeout=wait_seconds)
            except queue.Empty:
                continue
            self._dispatch_event(event, output, ended_streams, stdout_handler)

        for reader in readers:
            reader.join(timeout=1.0)
        self._active_tree = None
        return (
            exit_status if exit_status is not None else _CLEANUP_EXIT,
            "".join(output["stdout"]),
            "".join(output["stderr"]),
        )

    def _emit_diagnostics(
        self,
        tree: OwnedProcessTree,
        command: list[str],
        cwd: pathlib.Path,
        environment: dict[str, str],
        started_at: float,
        reason: str,
    ) -> None:
        """Write the process and known-lock report before cleanup begins."""
        report = format_stall_report(
            tree,
            command=tuple(command),
            cwd=cwd,
            environment=environment,
            target_directory=(
                pathlib.Path(environment["CARGO_TARGET_DIR"])
                if environment.get("CARGO_TARGET_DIR")
                else None
            ),
            elapsed_seconds=time.monotonic() - self.started_at,
            timeout_seconds=self.timeout_seconds,
            reason=reason,
        )
        print(report, file=sys.stderr, flush=True)

    @staticmethod
    def _terminate_tree(tree: OwnedProcessTree, process: subprocess.Popen[str]) -> None:
        """Clean only the current process tree and report any failed reap."""
        if not tree.terminate(process):
            print(
                "test runner: cleanup did not reap the complete owned process tree",
                file=sys.stderr,
                flush=True,
            )

    def _record_signal(self, signum: int, _frame: typ.Any) -> None:
        """Defer signal cleanup to the process-monitoring loop."""
        self._received_signal = signum

    def _drain_events(
        self,
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
            self._dispatch_event(event, output, ended_streams, stdout_handler)

    @staticmethod
    def _dispatch_event(
        event: tuple[str, str | object],
        output: dict[str, list[str]],
        ended_streams: set[str],
        stdout_handler: typ.Callable[[str], None] | None,
    ) -> None:
        """Forward captured chunks and record pipe EOF notifications."""
        stream_name, chunk = event
        if chunk is _READ_END:
            ended_streams.add(stream_name)
        elif isinstance(chunk, str):
            if stream_name == "stdout" and stdout_handler is not None:
                stdout_handler(chunk)
            else:
                output[stream_name].append(chunk)

    def _drain_after_cleanup(
        self,
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
            self._dispatch_event(event, output, ended_streams, stdout_handler)


def _piped_stream_names(stdout_pipe: bool, stderr_pipe: bool) -> tuple[str, ...]:
    """Return the standard streams that need reader threads."""
    return tuple(
        name
        for name, enabled in (("stdout", stdout_pipe), ("stderr", stderr_pipe))
        if enabled
    )


def _read_stream(
    stream_name: str,
    stream: typ.TextIO,
    events: queue.Queue[tuple[str, str | object]],
) -> None:
    """Move child output to the supervising thread without blocking its timer."""
    try:
        for line in stream:
            events.put((stream_name, line))
    finally:
        events.put((stream_name, _READ_END))


def _process_group_arguments() -> dict[str, int | bool]:
    """Create a private process group for descendants launched by this runner."""
    if os.name == "posix":
        return {"start_new_session": True}
    return {"creationflags": getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)}


def _normal_exit_status(return_code: int) -> int:
    """Convert subprocess signal exits to the conventional shell status."""
    return return_code if return_code >= 0 else 128 - return_code
