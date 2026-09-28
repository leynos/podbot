"""Run Cargo and test processes under one deadline and owned-tree policy."""

from __future__ import annotations

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
from test_runner_process_io import (
    dispatch_event as _dispatch_event,
    drain_after_cleanup as _drain_after_cleanup,
    drain_events as _drain_events,
    join_readers as _join_readers,
    normal_exit_status as _normal_exit_status,
    piped_stream_names as _piped_stream_names,
    process_group_arguments as _process_group_arguments,
    read_stream as _read_stream,
)
from test_runner_process_tree import (
    OwnedProcessTree,
    enable_child_subreaper,
    get_child_subreaper,
    snapshot_direct_child_identities,
    set_child_subreaper,
)

_TIMEOUT_EXIT = 124
_CLEANUP_EXIT = 125


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
        preexisting_child_identities = (
            snapshot_direct_child_identities()
            if self.subreaper_enabled
            else frozenset()
        )
        try:
            process = self._start_process(
                command, cwd, environment, stdout_pipe, stderr_pipe
            )
        except OSError as exc:
            print(f"test runner: could not start {command[0]}: {exc}", file=sys.stderr)
            return 127, "", ""
        tree = OwnedProcessTree(
            process.pid,
            shlex.join(command),
            started_at,
            subreaper=self.subreaper_enabled,
            preexisting_child_identities=preexisting_child_identities,
        )
        events, readers, expected_streams = self._start_readers(
            process, stdout_pipe, stderr_pipe
        )
        return self._monitor_process(
            process,
            tree,
            command,
            cwd,
            environment,
            purpose,
            started_at,
            events,
            readers,
            expected_streams,
            stdout_handler,
        )

    @staticmethod
    def _start_process(
        command: list[str],
        cwd: pathlib.Path,
        environment: dict[str, str],
        stdout_pipe: bool,
        stderr_pipe: bool,
    ) -> subprocess.Popen[str]:
        """Start one command in a private process group with selected pipes."""
        return subprocess.Popen(
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

    @staticmethod
    def _start_readers(
        process: subprocess.Popen[str], stdout_pipe: bool, stderr_pipe: bool
    ) -> tuple[
        queue.Queue[tuple[str, str | object]],
        list[threading.Thread],
        set[str],
    ]:
        """Read piped child streams concurrently so monitoring remains bounded."""
        events: queue.Queue[tuple[str, str | object]] = queue.Queue()
        readers: list[threading.Thread] = []
        expected_streams = set(_piped_stream_names(stdout_pipe, stderr_pipe))
        for stream_name in expected_streams:
            stream = process.stdout if stream_name == "stdout" else process.stderr
            if stream is None:
                continue
            reader = threading.Thread(
                target=_read_stream,
                args=(stream_name, stream, events),
                name=f"test-runner-{stream_name}-{process.pid}",
                daemon=True,
            )
            reader.start()
            readers.append(reader)
        return events, readers, expected_streams

    def _monitor_process(
        self,
        process: subprocess.Popen[str],
        tree: OwnedProcessTree,
        command: list[str],
        cwd: pathlib.Path,
        environment: dict[str, str],
        purpose: str,
        started_at: float,
        events: queue.Queue[tuple[str, str | object]],
        readers: list[threading.Thread],
        expected_streams: set[str],
        stdout_handler: typ.Callable[[str], None] | None,
    ) -> tuple[int, str, str]:
        """Coordinate output, deadline checks, diagnostics, and final status."""
        output: dict[str, list[str]] = {"stdout": [], "stderr": []}
        ended_streams: set[str] = set()
        next_watch = time.monotonic() + self.watch_interval_seconds
        exit_status: int | None = None
        while True:
            _drain_events(events, output, ended_streams, stdout_handler)
            tree.refresh()
            tree.reap_adopted()
            return_status = process.poll()
            terminal_reason = self._terminal_reason(tree, process, return_status)
            if terminal_reason is not None:
                self._emit_diagnostics(
                    tree, command, cwd, environment, started_at, terminal_reason
                )
                self._terminate_tree(tree, process)
                _drain_after_cleanup(
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
            event = self._wait_for_event(events, now, next_watch)
            if event is not None:
                _dispatch_event(event, output, ended_streams, stdout_handler)
        _join_readers(readers)
        return (
            exit_status if exit_status is not None else _CLEANUP_EXIT,
            "".join(output["stdout"]),
            "".join(output["stderr"]),
        )

    def _terminal_reason(
        self,
        tree: OwnedProcessTree,
        process: subprocess.Popen[str],
        return_status: int | None,
    ) -> str | None:
        """Set the terminal status for signals, deadline, or leaked descendants."""
        if self.terminal_status is not None:
            return "supervision already stopped"
        if self._received_signal is not None:
            signum = self._received_signal
            self.terminal_status = 128 + signum
            return f"interrupted by {signal.Signals(signum).name}"
        if time.monotonic() >= self.deadline:
            self.terminal_status = _TIMEOUT_EXIT
            return "timed out"
        if return_status is not None and tree.descendants():
            self.terminal_status = _CLEANUP_EXIT
            return "command exited while owned descendants remained"
        return None

    def _wait_for_event(
        self,
        events: queue.Queue[tuple[str, str | object]],
        now: float,
        next_watch: float,
    ) -> tuple[str, str | object] | None:
        """Wait briefly for output, a signal, or the next timer check."""
        wait_seconds = min(0.1, max(0.0, self.deadline - now))
        wait_seconds = min(wait_seconds, max(0.0, next_watch - now))
        try:
            return events.get(timeout=wait_seconds)
        except queue.Empty:
            return None

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
        try:
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
        except Exception as exc:
            report = f"test runner: {reason}; diagnostics unavailable: {exc}"
        try:
            print(report, file=sys.stderr, flush=True)
        except OSError:
            pass

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
