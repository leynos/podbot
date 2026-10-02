"""Cover process startup and Windows job handoff in the supervisor."""

from __future__ import annotations

import pathlib
import queue

import pytest
import test_runner_supervisor as supervisor_module
from test_runner_process_io import ProcessLaunchRequest, StreamCapture
from test_runner_supervisor import CommandRequest, ProcessSupervisor


class FakeProcess:
    """Record cleanup calls made to a launched process."""

    def __init__(self, calls: list[str]) -> None:
        self.pid = 101
        self._handle = 202
        self.calls = calls

    def kill(self) -> None:
        self.calls.append("kill")

    def wait(self) -> None:
        self.calls.append("wait")


class FakeWindowsJob:
    """Record assignment and closure without requiring Windows APIs."""

    def __init__(
        self,
        calls: list[str],
        *,
        assigned: bool = True,
        assignment_error: OSError | None = None,
    ) -> None:
        self.calls = calls
        self.assigned = assigned
        self.assignment_error = assignment_error

    def assign_and_resume(self, process_handle: int) -> bool:
        self.calls.append(f"assign:{process_handle}")
        if self.assignment_error is not None:
            raise self.assignment_error
        return self.assigned

    def close(self) -> None:
        self.calls.append("close")


def _request(tmp_path: pathlib.Path) -> CommandRequest:
    """Build a request whose fields can be checked by identity."""
    return CommandRequest(
        ["cargo", "test"],
        tmp_path,
        {"CARGO_HOME": str(tmp_path / "cargo-home")},
        "supervisor launch test",
    )


def _install_launcher(
    monkeypatch: pytest.MonkeyPatch,
    calls: list[str],
    job: FakeWindowsJob | None,
    *,
    start_error: OSError | None = None,
) -> tuple[FakeProcess, list[ProcessLaunchRequest]]:
    """Install portable fakes for job creation and the single-request launcher."""
    process = FakeProcess(calls)
    launch_requests: list[ProcessLaunchRequest] = []

    def create_job(_job_type: type[object]) -> FakeWindowsJob | None:
        calls.append("create")
        return job

    def start(request: ProcessLaunchRequest) -> FakeProcess:
        calls.append("start")
        launch_requests.append(request)
        if start_error is not None:
            raise start_error
        return process

    monkeypatch.setattr(
        supervisor_module.WindowsProcessJob, "create", classmethod(create_job)
    )
    monkeypatch.setattr(supervisor_module, "_start_process", start)
    return process, launch_requests


@pytest.mark.parametrize(
    "launch_case",
    [
        pytest.param(
            (False, True, False, ["create", "start"]),
            id="without-job",
        ),
        pytest.param(
            (True, False, True, ["create", "start", "assign:202"]),
            id="assigned-job",
        ),
    ],
)
def test_launch_forwards_command_and_pipe_settings(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    launch_case: tuple[bool, bool, bool, list[str]],
) -> None:
    """A job-backed child is assigned; a jobless child starts unsuspended."""
    has_job, stdout_pipe, stderr_pipe, expected_calls = launch_case
    calls: list[str] = []
    job = FakeWindowsJob(calls) if has_job else None
    process, launch_requests = _install_launcher(monkeypatch, calls, job)
    request = _request(tmp_path)

    result = ProcessSupervisor(10)._launch_process(request, stdout_pipe, stderr_pipe)

    assert result == (process, job), "launch must return the process and surviving job"
    assert calls == expected_calls, "launch and assignment calls must keep their order"
    launch_request = launch_requests[0]
    assert launch_request.command is request.command, "the command must be forwarded"
    assert launch_request.cwd is request.cwd, "the working directory must be forwarded"
    assert launch_request.environment is request.environment, (
        "the caller's environment object must be forwarded"
    )
    assert launch_request.stdout_pipe is stdout_pipe, (
        "stdout selection must be forwarded independently"
    )
    assert launch_request.stderr_pipe is stderr_pipe, (
        "stderr selection must be forwarded independently"
    )
    assert launch_request.suspended is has_job, (
        "a child must start suspended only when a job exists"
    )


def test_assignment_failure_closes_job_and_keeps_resumed_process(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A false assignment closes the job but continues with the resumed child."""
    calls: list[str] = []
    job = FakeWindowsJob(calls, assigned=False)
    process, launch_requests = _install_launcher(monkeypatch, calls, job)
    request = _request(tmp_path)

    result = ProcessSupervisor(10)._launch_process(request, False, False)

    assert result == (process, None), "a false assignment must keep the child running"
    assert calls == ["create", "start", "assign:202", "close"], (
        "a failed assignment must close the job after its single resume operation"
    )
    assert launch_requests[0].suspended is True, (
        "the job-backed child must be suspended for the assignment attempt"
    )


@pytest.mark.parametrize("with_job", [False, True], ids=["without-job", "with-job"])
def test_spawn_oserror_returns_none_closes_job_and_reports_exact_diagnostic(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    with_job: bool,
) -> None:
    """Spawn errors close only an acquired job and keep the diagnostic exact."""
    calls: list[str] = []
    job = FakeWindowsJob(calls) if with_job else None
    process, launch_requests = _install_launcher(
        monkeypatch,
        calls,
        job,
        start_error=OSError("spawn refused"),
    )
    request = _request(tmp_path)

    result = ProcessSupervisor(10)._launch_process(request, True, False)

    assert result is None, "a handled spawn failure must return no process"
    expected_calls = ["create", "start"] + (["close"] if with_job else [])
    assert calls == expected_calls, "spawn failure cleanup must follow ownership"
    assert launch_requests[0].command is request.command, (
        "the command must be forwarded"
    )
    assert launch_requests[0].cwd is request.cwd, (
        "the working directory must be forwarded"
    )
    assert launch_requests[0].environment is request.environment, (
        "the caller's environment object must be forwarded"
    )
    assert launch_requests[0].suspended is with_job, (
        "only a job-backed process may be suspended"
    )
    assert capsys.readouterr().err == (
        "test runner: could not start cargo: spawn refused\n"
    ), "spawn failures must keep the existing diagnostic"


def test_containment_oserror_closes_job_then_kills_and_waits(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    """A containment error closes the job before killing and reaping the child."""
    calls: list[str] = []
    job = FakeWindowsJob(
        calls,
        assignment_error=OSError("assignment refused"),
    )
    process, launch_requests = _install_launcher(monkeypatch, calls, job)
    request = _request(tmp_path)

    result = ProcessSupervisor(10)._launch_process(request, True, True)

    assert result is None, "a handled containment error must return no process"
    assert calls == ["create", "start", "assign:202", "close", "kill", "wait"], (
        "containment cleanup must close, kill, and wait in order"
    )
    assert launch_requests[0].suspended is True, (
        "containment must follow a suspended job-backed launch"
    )
    assert capsys.readouterr().err == (
        "test runner: could not contain cargo: assignment refused\n"
    ), "containment failures must keep the existing diagnostic"


def test_windows_job_creation_oserror_keeps_existing_failure_policy(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Job creation errors propagate before the process launcher is called."""
    calls: list[str] = []
    _, launch_requests = _install_launcher(monkeypatch, calls, None)

    def fail_create(_job_type: type[object]) -> FakeWindowsJob | None:
        calls.append("create")
        raise OSError("job creation failed")

    monkeypatch.setattr(
        supervisor_module.WindowsProcessJob, "create", classmethod(fail_create)
    )
    with pytest.raises(OSError, match="job creation failed"):
        ProcessSupervisor(10)._launch_process(_request(tmp_path), False, False)

    assert calls == ["create"], "job creation failure must prevent process launch"
    assert launch_requests == [], "the launch request must not be constructed for spawn"


def test_run_maps_handled_launch_failure_to_status_127(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A handled launch failure becomes the established empty status result."""
    supervisor = ProcessSupervisor(10)
    request = _request(tmp_path)
    launch_calls: list[tuple[CommandRequest, bool, bool]] = []

    def fail_launch(
        selected_request: CommandRequest,
        stdout_pipe: bool,
        stderr_pipe: bool,
    ) -> None:
        launch_calls.append((selected_request, stdout_pipe, stderr_pipe))
        return None

    monkeypatch.setattr(supervisor, "_launch_process", fail_launch)

    result = supervisor.run_capture(request)

    assert result == (127, "", ""), (
        "handled launch failures must return empty status 127"
    )
    assert launch_calls == [(request, True, True)], (
        "run_capture must forward its request and both pipe settings"
    )


@pytest.mark.parametrize(
    "monitor_error",
    [
        pytest.param(None, id="normal-monitoring"),
        pytest.param(True, id="monitor-error"),
    ],
)
def test_successful_job_closes_after_monitoring(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    monitor_error: bool | None,
) -> None:
    """The run owns a successful job until monitoring returns or raises."""
    calls: list[str] = []
    job = FakeWindowsJob(calls)
    process, _ = _install_launcher(monkeypatch, calls, job)
    supervisor = ProcessSupervisor(10)

    def start_readers(
        selected_process: FakeProcess,
        stdout_pipe: bool,
        stderr_pipe: bool,
        stdout_handler: object,
    ) -> tuple[queue.Queue[tuple[str, str | object]], list[object], StreamCapture]:
        assert selected_process is process, "reader setup must use the launched process"
        assert stdout_pipe is True, "run_capture must pipe stdout"
        assert stderr_pipe is True, "run_capture must pipe stderr"
        assert stdout_handler is None, "run_capture must not add a line handler"
        calls.append("readers")
        return queue.Queue(), [], StreamCapture({"stdout", "stderr"})

    def monitor(
        _supervisor: ProcessSupervisor,
        _run: supervisor_module._ProcessRun,
    ) -> tuple[int, str, str]:
        calls.append("monitor")
        if monitor_error:
            raise RuntimeError("monitor failed")
        return 0, "stdout", "stderr"

    monkeypatch.setattr(supervisor_module, "_start_readers", start_readers)
    monkeypatch.setattr(ProcessSupervisor, "_monitor_process", monitor)

    if monitor_error:
        with pytest.raises(RuntimeError, match="monitor failed"):
            supervisor.run_capture(_request(tmp_path))
    else:
        result = supervisor.run_capture(_request(tmp_path))
        assert result == (0, "stdout", "stderr"), (
            "normal monitoring output must pass through the supervisor"
        )

    assert calls == ["create", "start", "assign:202", "readers", "monitor", "close"], (
        "the successful job must close after monitoring in both outcomes"
    )


def test_existing_terminal_status_prevents_job_creation_and_launch(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A previously selected terminal status prevents a second child launch."""
    calls: list[str] = []
    supervisor = ProcessSupervisor(10)
    supervisor.terminal_status = 143

    def create_job(_job_type: type[object]) -> FakeWindowsJob | None:
        calls.append("create")
        return FakeWindowsJob(calls)

    def start(_request: ProcessLaunchRequest) -> FakeProcess:
        calls.append("start")
        return FakeProcess(calls)

    monkeypatch.setattr(
        supervisor_module.WindowsProcessJob, "create", classmethod(create_job)
    )
    monkeypatch.setattr(supervisor_module, "_start_process", start)

    result = supervisor.run_capture(_request(tmp_path))

    assert result == (143, "", ""), "the existing terminal result must be preserved"
    assert calls == [], "terminal state must prevent job creation and process launch"
