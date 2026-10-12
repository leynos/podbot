"""Specify metadata failure propagation to the test runner boundary."""

from __future__ import annotations

import pathlib
from typing import TYPE_CHECKING, cast

import pytest
import test_runner
import test_runner_cargo
from test_runner_cargo import RunnerCommandFailure
from test_runner_context import TestRunnerContext
from test_runner_options import parse_cargo_test_options
from test_runner_supervisor import CommandRequest

if TYPE_CHECKING:
    from test_runner_supervisor import ProcessSupervisor


class FailedMetadataSupervisor:
    """Return a captured metadata failure without rendering it."""

    def __init__(self, terminal_status: int | None, diagnostic: str) -> None:
        self.terminal_status = terminal_status
        self.diagnostic = diagnostic

    def __enter__(self) -> FailedMetadataSupervisor:
        return self

    def __exit__(self, *_args: object) -> None:
        return None

    def run_capture(self, request: CommandRequest) -> tuple[int, str, str]:
        del request
        return 17, "", self.diagnostic


@pytest.mark.parametrize(
    ("terminal_status", "expected_status", "expected_summary"),
    (
        (None, 2, "cargo metadata exited with status 17"),
        (23, 23, None),
    ),
    ids=("cargo-failure", "supervisor-terminal-status"),
)
def test_metadata_failure_is_structured_without_output(
    capsys: pytest.CaptureFixture[str],
    terminal_status: int | None,
    expected_status: int,
    expected_summary: str | None,
) -> None:
    """Metadata discovery retains status and diagnostics without writing."""
    supervisor = FailedMetadataSupervisor(
        terminal_status, "cargo metadata failed TOKEN=raw-metadata-value\n"
    )
    context = TestRunnerContext(
        ("cargo",), pathlib.Path.cwd(), {}, cast("ProcessSupervisor", supervisor)
    )
    options = parse_cargo_test_options([], cwd=pathlib.Path.cwd())

    with pytest.raises(RunnerCommandFailure) as raised:
        test_runner_cargo.load_cargo_metadata(context, options)

    assert capsys.readouterr() == ("", ""), "query helpers must not render errors"
    assert raised.value.status == expected_status, "preserve the runner status"
    assert raised.value.diagnostic == (
        "cargo metadata failed TOKEN=raw-metadata-value\n"
    ), "carry captured stderr intact to the reporting boundary"
    assert raised.value.summary == expected_summary, "preserve failure context"


def test_runner_renders_structured_metadata_failure_once(
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The outer runner redacts and emits structured metadata failure once."""
    supervisor = FailedMetadataSupervisor(
        terminal_status=None,
        diagnostic="cargo metadata failed TOKEN=raw-metadata-value\n",
    )

    def make_supervisor(*_args: object, **_kwargs: object) -> FailedMetadataSupervisor:
        return supervisor

    monkeypatch.setattr(test_runner, "ProcessSupervisor", make_supervisor)
    monkeypatch.setattr(test_runner, "start_configured_sccache", lambda _env: None)

    status = test_runner.main(["--cargo", "cargo"])

    assert status == 2, "metadata failures retain the runner's ordinary status"
    assert capsys.readouterr().err == (
        "cargo metadata failed TOKEN=<redacted>\n"
        "test runner: cargo metadata exited with status 17\n"
    ), "render captured stderr and summary once at the outer boundary"


def test_runner_redacts_multiline_environment_secrets_before_line_filtering(
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A multiline environment secret cannot leak fragments in captured stderr."""
    secret = "first-secret-fragment\nsecond-secret-fragment"
    supervisor = FailedMetadataSupervisor(
        terminal_status=None,
        diagnostic=f"cargo metadata failed {secret}\n",
    )

    def make_supervisor(*_args: object, **_kwargs: object) -> FailedMetadataSupervisor:
        return supervisor

    monkeypatch.setenv("API_TOKEN", secret)
    monkeypatch.setattr(test_runner, "ProcessSupervisor", make_supervisor)
    monkeypatch.setattr(test_runner, "start_configured_sccache", lambda _env: None)

    status = test_runner.main(["--cargo", "cargo"])

    assert status == 2, "metadata failure status must remain unchanged"
    assert capsys.readouterr().err == (
        "cargo metadata failed <redacted>\n"
        "<redacted>\n"
        "test runner: cargo metadata exited with status 17\n"
    ), "redact whole secret values while retaining captured line boundaries"
