"""Exercise bounded cleanup, subreaper ownership, and lock diagnostics."""

from __future__ import annotations

import os
import pathlib
import signal
import subprocess
import sys
import textwrap
import time
import fcntl

import pytest

from test_runner_diagnostics import classify_lock_waiter, parse_proc_locks
from test_runner_process_tree import parse_proc_stat


@pytest.mark.parametrize("terminator", ["timeout", "signal"])
@pytest.mark.skipif(sys.platform != "linux", reason="procfs cleanup test is Linux-only")
def test_supervisor_reaps_escaped_descendant_and_preserves_unrelated_process(
    tmp_path: pathlib.Path, terminator: str
) -> None:
    """Timeout and interruption clean an escaped grandchild, not a neighbour."""
    descendant_file = tmp_path / "descendant.pid"
    unrelated = subprocess.Popen(
        [sys.executable, "-c", "import time; time.sleep(30)"],
        start_new_session=True,
    )
    scripts_directory = pathlib.Path(__file__).resolve().parents[1]
    python_path = os.pathsep.join(
        path
        for path in (str(scripts_directory), os.environ.get("PYTHONPATH", ""))
        if path
    )
    helper = subprocess.Popen(
        [sys.executable, "-c", _supervisor_helper(terminator)],
        cwd=pathlib.Path.cwd(),
        env={
            **os.environ,
            "DESCENDANT_PID_FILE": str(descendant_file),
            "PYTHONPATH": python_path,
        },
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
    )
    try:
        if terminator == "signal":
            _wait_for_file(descendant_file)
            helper.send_signal(signal.SIGTERM)
        stdout, stderr = helper.communicate(timeout=10)
        assert helper.returncode == 0, "the supervisor helper must finish cleanly"
        expected_status = 124 if terminator == "timeout" else 143
        assert f"STATUS={expected_status}" in stdout, (
            "timeout and SIGTERM must have distinct, non-zero statuses"
        )
        assert "owned process tree" in stderr, (
            "cleanup must emit its process report before terminating children"
        )
        expected_diagnostic = (
            "test runner: timed out"
            if terminator == "timeout"
            else "test runner: interrupted by SIGTERM"
        )
        assert expected_diagnostic in stderr, (
            "diagnostics must identify timeout versus interruption"
        )
        descendant_pid = int(descendant_file.read_text(encoding="utf-8"))
        assert not pathlib.Path(f"/proc/{descendant_pid}").exists(), (
            "the escaped grandchild must be gone before the supervisor exits"
        )
        assert unrelated.poll() is None, (
            "a process outside the runner-owned tree must remain alive"
        )
    finally:
        if helper.poll() is None:
            helper.kill()
            helper.wait(timeout=5)
        unrelated.terminate()
        unrelated.wait(timeout=5)


def test_proc_stat_parser_handles_parentheses_in_command_name() -> None:
    """The final command-name delimiter precedes the process fields."""
    fields = ["S", "41", "42", *["0"] * 16, "987654"]
    parsed = parse_proc_stat(
        f"123 (cargo (nested)) {' '.join(fields)}", "cargo --offline build"
    )

    assert parsed.pid == 123
    assert parsed.parent_pid == 41
    assert parsed.process_group == 42
    assert parsed.start_time == 987654
    assert parsed.command == "cargo --offline build"


def test_proc_locks_parser_classifies_parent_cycles_and_external_contention() -> None:
    """A wait owned by a descendant is distinct from shared-cache contention."""
    records = parse_proc_locks(
        "\n".join(
            (
                "1: FLOCK ADVISORY WRITE 100 08:01:10 0 EOF",
                "1: -> FLOCK ADVISORY READ 200 08:01:10 0 EOF",
            )
        )
    )
    holder, waiter = records

    assert waiter.waiter
    assert waiter.pid == 200
    assert waiter.device_inode == holder.device_inode
    assert classify_lock_waiter([holder], {100, 200}, {100}) == (
        "parent/descendant lock cycle"
    )
    assert classify_lock_waiter([holder], {200}, set()) == (
        "ordinary contention with an external lock holder"
    )


@pytest.mark.parametrize(
    ("holder_location", "diagnostic"),
    [
        ("owned-parent", "parent/descendant lock cycle"),
        ("external", "ordinary contention with an external lock holder"),
    ],
)
@pytest.mark.skipif(
    sys.platform != "linux", reason="/proc lock diagnostics are Linux-only"
)
def test_real_flock_wait_is_classified_from_proc_locks(
    tmp_path: pathlib.Path, holder_location: str, diagnostic: str
) -> None:
    """Real FLOCK waits distinguish internal cycles from external contention."""
    cargo_home = tmp_path / "cargo-home"
    cargo_home.mkdir()
    cache_lock = cargo_home / ".package-cache-mutate"
    cache_lock.touch()
    external_lock = cache_lock.open("r+")
    if holder_location == "external":
        fcntl.flock(external_lock, fcntl.LOCK_EX)
    helper_environment = _helper_environment(cargo_home)
    try:
        result = subprocess.run(
            [sys.executable, "-c", _lock_supervisor_helper(holder_location)],
            cwd=pathlib.Path.cwd(),
            env=helper_environment,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=10,
            check=False,
        )
    finally:
        external_lock.close()

    assert result.returncode == 0, "the bounded lock-wait helper must exit cleanly"
    assert "STATUS=124" in result.stdout, "a blocked lock wait must time out"
    assert diagnostic in result.stderr, (
        "the report must classify the lock relationship from procfs ownership"
    )
    assert "Cargo package cache mutation lock" in result.stderr, (
        "the report must map the lock inode back to the Cargo cache path"
    )


def _supervisor_helper(terminator: str) -> str:
    """Return a helper program isolated from pytest's process state."""
    timeout = "0.6" if terminator == "timeout" else "20"
    return textwrap.dedent(
        f"""\
        import os
        import pathlib
        import subprocess
        import sys
        from test_runner_supervisor import ProcessSupervisor

        marker = pathlib.Path(os.environ["DESCENDANT_PID_FILE"])
        child = (
            "import os, pathlib, subprocess, sys, time; "
            "grandchild = subprocess.Popen([sys.executable, '-c', "
            "'import time; time.sleep(60)'], start_new_session=True, "
            "stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, "
            "stderr=subprocess.DEVNULL); "
            "pathlib.Path(os.environ['DESCENDANT_PID_FILE']).write_text("
            "str(grandchild.pid)); time.sleep(60)"
        )
        with ProcessSupervisor({timeout}, 0.1, enable_subreaper=True) as supervisor:
            status = supervisor.run_inherited(
                [sys.executable, "-c", child],
                pathlib.Path.cwd(),
                os.environ.copy(),
                purpose="controlled cleanup test",
            )
        print(f"STATUS={{status}}")
        """
    )


def _lock_supervisor_helper(holder_location: str) -> str:
    """Return a helper that blocks on the shared Cargo package-cache lock."""
    if holder_location == "owned-parent":
        command_body = textwrap.dedent(
            """\
            import fcntl, subprocess, sys, time
            lock = open(sys.argv[1], "r+")
            fcntl.flock(lock, fcntl.LOCK_EX)
            waiter = (
                "import fcntl, sys; "
                "lock = open(sys.argv[1], 'r+'); "
                "fcntl.flock(lock, fcntl.LOCK_SH)"
            )
            subprocess.Popen(
                [sys.executable, "-c", waiter, sys.argv[1]],
                start_new_session=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            time.sleep(60)
            """
        )
    else:
        command_body = textwrap.dedent(
            """\
            import fcntl, sys, time
            lock = open(sys.argv[1], "r+")
            fcntl.flock(lock, fcntl.LOCK_SH)
            time.sleep(60)
            """
        )
    return textwrap.dedent(
        f"""\
        import os
        import pathlib
        import sys
        from test_runner_supervisor import ProcessSupervisor

        command_body = {command_body!r}
        cache_lock = pathlib.Path(os.environ["CARGO_HOME"]) / ".package-cache-mutate"
        with ProcessSupervisor(1.2, 2, enable_subreaper=True) as supervisor:
            status = supervisor.run_inherited(
                [sys.executable, "-c", command_body, str(cache_lock)],
                pathlib.Path.cwd(),
                os.environ.copy(),
                purpose="Cargo package-cache lock test",
            )
        print(f"STATUS={{status}}")
        """
    )


def _helper_environment(cargo_home: pathlib.Path) -> dict[str, str]:
    """Add script imports and a private Cargo home for subprocess fixtures."""
    scripts_directory = pathlib.Path(__file__).resolve().parents[1]
    python_path = os.pathsep.join(
        path
        for path in (str(scripts_directory), os.environ.get("PYTHONPATH", ""))
        if path
    )
    return {
        **os.environ,
        "CARGO_HOME": str(cargo_home),
        "PYTHONPATH": python_path,
    }


def _wait_for_file(path: pathlib.Path) -> None:
    """Wait briefly for a fixture child to publish its PID."""
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if path.exists():
            return
        time.sleep(0.02)
    raise AssertionError("fixture grandchild did not publish its PID")
