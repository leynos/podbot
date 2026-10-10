"""Test best-effort sccache startup before child process supervision."""

from __future__ import annotations

import os
import pathlib
import signal
import subprocess
import sys
import textwrap

import pytest
import test_runner_sccache as sccache_module
from test_runner_supervisor import CommandRequest, ProcessSupervisor

_FAKE_SCCACHE_SCRIPT = textwrap.dedent(
    """\
    import os
    import pathlib
    import subprocess
    import sys

    state = pathlib.Path(os.environ["FAKE_SCCACHE_STATE_FILE"])
    if "--start-server" in sys.argv:
        state.touch()
    elif (
        "--simulate-idle" in sys.argv
        and os.environ.get("SCCACHE_IDLE_TIMEOUT") != "0"
    ):
        state.unlink(missing_ok=True)
    elif "--compile" in sys.argv and not state.exists():
        server = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(60)"],
            start_new_session=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        pathlib.Path(os.environ["FAKE_SCCACHE_PID_FILE"]).write_text(
            str(server.pid)
        )
        state.touch()
    """
)


@pytest.mark.parametrize(
    ("wrapper", "expected"),
    [
        ("/usr/bin/sccache", ("/usr/bin/sccache", "--start-server")),
        (
            "/opt/cache/sccache --cache-size 2G",
            ("/opt/cache/sccache", "--cache-size", "2G", "--start-server"),
        ),
        ("ccache", None),
        ("", None),
    ],
    ids=["path", "arguments", "other-wrapper", "unset"],
)
def test_sccache_start_command_detects_only_sccache_wrappers(
    wrapper: str, expected: tuple[str, ...] | None
) -> None:
    """Start-server is added only when Cargo names sccache as its wrapper."""
    assert (
        sccache_module.configured_sccache_start_command({"RUSTC_WRAPPER": wrapper})
        == expected
    ), "only an sccache wrapper should receive the server-start command"


def test_sccache_start_disables_server_idle_shutdown(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Set server lifetime before invoking the configured wrapper."""
    environment = {"RUSTC_WRAPPER": "sccache"}
    captured: dict[str, object] = {}

    def capture_run(command: tuple[str, ...], **kwargs: object) -> None:
        captured["command"] = command
        captured["environment"] = kwargs["env"]
        captured["idle_timeout"] = environment.get("SCCACHE_IDLE_TIMEOUT")

    monkeypatch.setattr(sccache_module.subprocess, "run", capture_run)

    sccache_module.start_configured_sccache(environment)

    assert environment["SCCACHE_IDLE_TIMEOUT"] == "0", (
        "the prestarted server must not exit before later Cargo phases"
    )
    assert captured["idle_timeout"] == "0", (
        "server lifetime must be configured before startup"
    )
    assert captured["command"] == ("sccache", "--start-server"), (
        "the configured wrapper must start the sccache server"
    )
    assert captured["environment"] is environment, (
        "startup must receive the same environment later used by Cargo"
    )


@pytest.mark.skipif(sys.platform != "linux", reason="subreaper ownership is Linux-only")
def test_prestarted_sccache_server_does_not_idle_restart_inside_supervision(
    tmp_path: pathlib.Path,
) -> None:
    """A prestarted server cannot expire and restart as an owned descendant."""
    state_file = tmp_path / "sccache-server.state"
    pid_file = tmp_path / "sccache-server.pid"
    wrapper = tmp_path / "sccache"
    wrapper.write_text(
        f"#!{sys.executable}\n{_FAKE_SCCACHE_SCRIPT}",
        encoding="utf-8",
    )
    wrapper.chmod(0o755)
    environment = os.environ.copy()
    environment.update(
        RUSTC_WRAPPER=str(wrapper),
        FAKE_SCCACHE_STATE_FILE=str(state_file),
        FAKE_SCCACHE_PID_FILE=str(pid_file),
    )
    try:
        sccache_module.start_configured_sccache(environment)
        statuses = []
        with ProcessSupervisor(5, 0.1, enable_subreaper=True) as supervisor:
            for phase in ("--simulate-idle", "--compile"):
                statuses.append(
                    supervisor.run_inherited(
                        CommandRequest(
                            [str(wrapper), phase],
                            pathlib.Path.cwd(),
                            environment,
                            f"fake Cargo {phase}",
                        )
                    )
                )

        assert statuses == [0, 0], "both phases must avoid a false leaked-server status"
        assert state_file.exists() and not pid_file.exists(), (
            "later phases must reuse the prestarted fake server"
        )
    finally:
        if pid_file.exists():
            try:
                os.kill(int(pid_file.read_text(encoding="utf-8")), signal.SIGTERM)
            except ProcessLookupError:
                pass


@pytest.mark.parametrize(
    "failure",
    [OSError("sccache is missing"), subprocess.TimeoutExpired("sccache", 5)],
    ids=["missing", "unresponsive"],
)
def test_sccache_start_failures_are_best_effort(
    monkeypatch: pytest.MonkeyPatch, failure: Exception
) -> None:
    """Missing or unresponsive sccache startup never blocks the test runner."""

    def raise_failure(*_args: object, **_kwargs: object) -> None:
        raise failure

    monkeypatch.setattr(sccache_module.subprocess, "run", raise_failure)

    sccache_module.start_configured_sccache({"RUSTC_WRAPPER": "sccache"})
