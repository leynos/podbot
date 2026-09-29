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
from dataclasses import dataclass

from test_runner_diagnostics import StallReportContext, format_stall_report
from test_runner_process_io import (
    StreamCapture,
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
    ProcessTreeRoot,
    enable_child_subreaper,
    get_child_subreaper,
    snapshot_direct_child_identities,
    set_child_subreaper,
)

_TIMEOUT_EXIT = 124
_CLEANUP_EXIT = 125
_PROCESS_TREE_REFRESH_SECONDS = 1.0


@dataclass(frozen=True)
class CommandRequest:
    """Describe one child command launched by the test-runner supervisor.

    Keep argv, working directory, environment, and diagnostic purpose together
    from the Cargo phase builder through process launch and stall reporting.
    """

    command: list[str]
    cwd: pathlib.Path
    environment: dict[str, str]
    purpose: str


@dataclass(frozen=True)
class _OutputPolicy:
    """Select the streams and callback used for one supervised child."""

    capture_stdout: bool = False
    capture_stderr: bool = False
    stdout_handler: typ.Callable[[str], None] | None = None


@dataclass
class _ProcessRun:
    """Hold the mutable I/O and ownership state for one running child."""

    request: CommandRequest
    process: subprocess.Popen[str]
    tree: OwnedProcessTree
    events: queue.Queue[tuple[str, str | object]]
    readers: list[threading.Thread]
    capture: StreamCapture


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
        self.terminal_reason: str | None = None
        self._received_signal: int | None = None
        self._saved_handlers: dict[int, typ.Any] = {}
        self._previous_subreaper: bool | None = None

    def __enter__(self) -> ProcessSupervisor:
        """Install temporary signal handling and enable Linux orphan adoption."""
        self._install_signal_handlers()
        self._configure_subreaper()
        return self

    def _install_signal_handlers(self) -> None:
        """Temporarily handle interrupts on the runner's main thread."""
        if threading.current_thread() is not threading.main_thread():
            return
        for signum in (signal.SIGINT, signal.SIGTERM):
            self._saved_handlers[signum] = signal.getsignal(signum)
            signal.signal(signum, self._record_signal)

    def _configure_subreaper(self) -> None:
        """Enable orphan adoption when requested and supported by the host."""
        if not self.enable_subreaper:
            return
        self._previous_subreaper = get_child_subreaper()
        self.subreaper_enabled = enable_child_subreaper()
        if self.subreaper_enabled or sys.platform == "win32":
            return
        print(
            "test runner: Linux child-subreaper support is unavailable; "
            "process-group cleanup remains enabled",
            file=sys.stderr,
            flush=True,
        )

    def __exit__(self, *_: object) -> None:
        """Restore the caller's signal handlers after all children are reaped."""
        for signum, handler in self._saved_handlers.items():
            signal.signal(signum, handler)
        if self._previous_subreaper is not None:
            set_child_subreaper(self._previous_subreaper)

    def run_inherited(
        self,
        request: CommandRequest,
    ) -> int:
        """Run one command with inherited standard streams."""
        status, _, _ = self._run(request)
        return status

    def run_capture(
        self,
        request: CommandRequest,
    ) -> tuple[int, str, str]:
        """Capture standard output and error while continuing to supervise."""
        return self._run(
            request, _OutputPolicy(capture_stdout=True, capture_stderr=True)
        )

    def run_lines(
        self,
        request: CommandRequest,
        on_line: typ.Callable[[str], None],
    ) -> int:
        """Stream one command's standard output through a line callback."""
        status, _, _ = self._run(request, _OutputPolicy(stdout_handler=on_line))
        return status

    def _run(
        self,
        request: CommandRequest,
        output_policy: _OutputPolicy | None = None,
    ) -> tuple[int, str, str]:
        """Launch, monitor, diagnose and reap one supervised command."""
        if self.terminal_status is not None:
            return self.terminal_status, "", ""
        policy = output_policy if output_policy is not None else _OutputPolicy()
        stdout_pipe = policy.capture_stdout or policy.stdout_handler is not None
        stderr_pipe = policy.capture_stderr
        started_at = time.monotonic()
        preexisting_child_identities = (
            snapshot_direct_child_identities()
            if self.subreaper_enabled
            else frozenset()
        )
        try:
            process = self._start_process(request, stdout_pipe, stderr_pipe)
        except OSError as exc:
            print(
                f"test runner: could not start {request.command[0]}: {exc}",
                file=sys.stderr,
            )
            return 127, "", ""
        tree = OwnedProcessTree(
            ProcessTreeRoot(
                process.pid,
                shlex.join(request.command),
                started_at,
                self.subreaper_enabled,
                preexisting_child_identities,
            )
        )
        events, readers, capture = self._start_readers(
            process, stdout_pipe, stderr_pipe, policy.stdout_handler
        )
        run = _ProcessRun(
            request,
            process,
            tree,
            events,
            readers,
            capture,
        )
        return self._monitor_process(run)

    @staticmethod
    def _start_process(
        request: CommandRequest,
        stdout_pipe: bool,
        stderr_pipe: bool,
    ) -> subprocess.Popen[str]:
        """Start one command in a private process group with selected pipes."""
        return subprocess.Popen(
            request.command,
            cwd=request.cwd,
            env=request.environment,
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
        process: subprocess.Popen[str],
        stdout_pipe: bool,
        stderr_pipe: bool,
        stdout_handler: typ.Callable[[str], None] | None,
    ) -> tuple[
        queue.Queue[tuple[str, str | object]],
        list[threading.Thread],
        StreamCapture,
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
        return events, readers, StreamCapture(expected_streams, stdout_handler)

    def _monitor_process(self, run: _ProcessRun) -> tuple[int, str, str]:
        """Coordinate output, deadline checks, diagnostics, and final status."""
        next_watch = time.monotonic() + self.watch_interval_seconds
        next_tree_refresh = time.monotonic() + _PROCESS_TREE_REFRESH_SECONDS
        exit_status: int | None = None
        while True:
            _drain_events(run.events, run.capture)
            return_status = run.process.poll()
            now = time.monotonic()
            if return_status is not None or now >= next_tree_refresh:
                run.tree.refresh()
                next_tree_refresh = now + _PROCESS_TREE_REFRESH_SECONDS
            run.tree.reap_adopted()
            terminal_reason = self._terminal_reason(
                run.tree, run.process, return_status
            )
            if terminal_reason is not None:
                self._emit_diagnostics(run.tree, run.request, terminal_reason)
                self._terminate_tree(run.tree, run.process)
                _drain_after_cleanup(
                    run.events,
                    run.capture,
                )
                exit_status = self.terminal_status
                break
            if (
                return_status is not None
                and run.capture.ended_streams >= run.capture.expected_streams
            ):
                exit_status = _normal_exit_status(return_status)
                break
            if now >= next_watch:
                self._emit_diagnostics(
                    run.tree,
                    run.request,
                    f"still running ({run.request.purpose})",
                )
                next_watch = now + self.watch_interval_seconds
            event = self._wait_for_event(run.events, now, next_watch)
            if event is not None:
                _dispatch_event(event, run.capture)
        _join_readers(run.readers)
        return (
            exit_status if exit_status is not None else _CLEANUP_EXIT,
            "".join(run.capture.output["stdout"]),
            "".join(run.capture.output["stderr"]),
        )

    def _terminal_reason(
        self,
        tree: OwnedProcessTree,
        process: subprocess.Popen[str],
        return_status: int | None,
    ) -> str | None:
        """Set the terminal status for signals, deadline, or leaked descendants."""
        if self.terminal_status is not None:
            return self.terminal_reason or "supervision already stopped"
        if self._received_signal is not None:
            signum = self._received_signal
            self.terminal_status = 128 + signum
            self.terminal_reason = f"interrupted by {signal.Signals(signum).name}"
            return self.terminal_reason
        if time.monotonic() >= self.deadline:
            self.terminal_status = _TIMEOUT_EXIT
            self.terminal_reason = "timed out"
            return self.terminal_reason
        if return_status is not None and tree.descendants(refresh=False):
            self.terminal_status = _CLEANUP_EXIT
            self.terminal_reason = "command exited while owned descendants remained"
            return self.terminal_reason
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
        request: CommandRequest,
        reason: str,
    ) -> None:
        """Write the process and known-lock report before cleanup begins."""
        try:
            report_context = StallReportContext(
                tree=tree,
                command=tuple(request.command),
                cwd=request.cwd,
                environment=request.environment,
                target_directory=(
                    pathlib.Path(request.environment["CARGO_TARGET_DIR"])
                    if request.environment.get("CARGO_TARGET_DIR")
                    else None
                ),
                elapsed_seconds=time.monotonic() - self.started_at,
                timeout_seconds=self.timeout_seconds,
                reason=reason,
            )
            report = format_stall_report(report_context)
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
