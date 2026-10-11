"""Check that diagnostic rendering consumes captured external observations."""

from __future__ import annotations

import pathlib
import types

import pytest
import test_runner_diagnostic_collection as collection
import test_runner_diagnostics as diagnostics
from test_runner_diagnostics import LockRecord, StallReportContext, format_stall_report
from test_runner_process_io import CommandRequest
from test_runner_process_tree import (
    OwnedProcess,
    ProcessInfo,
    ProcessTreeSnapshot,
)


def test_stall_report_formatting_uses_only_supplied_observations(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Formatting does not read time, toolchain files, procfs, or lock paths."""

    def unexpected_observation(*args: object, **kwargs: object) -> None:
        raise AssertionError("report formatting must not collect external state")

    fail_path = types.SimpleNamespace(
        Path=unexpected_observation, home=unexpected_observation
    )
    for module in (collection, diagnostics):
        monkeypatch.setattr(module, "pathlib", fail_path, raising=False)
        monkeypatch.setattr(
            module,
            "time",
            types.SimpleNamespace(monotonic=unexpected_observation),
            raising=False,
        )
        for name in (
            "_find_toolchain_file",
            "_read_lock_records",
            "_toolchain_file_line",
            "lock_path_identities",
        ):
            monkeypatch.setattr(module, name, unexpected_observation, raising=False)

    context = StallReportContext(
        process_snapshot=ProcessTreeSnapshot(1, True, {}, ()),
        command=("cargo", "test"),
        cwd=pathlib.Path("/workspace"),
        environment={},
        elapsed_seconds=2.0,
        timeout_seconds=30.0,
        reason="timed out",
        process_observed_at=100.0,
        toolchain_file_line="    toolchain file: /workspace/rust-toolchain.toml (channel=stable)",
        lock_records=(),
        lock_path_identities=(),
    )

    report = format_stall_report(context)

    assert report == "\n".join(
        (
            "test runner: timed out after 2.0s (deadline 30.0s)",
            "  command: cargo test",
            "  working directory: /workspace",
            "  toolchain/environment:",
            "    toolchain file: /workspace/rust-toolchain.toml (channel=stable)",
            "  owned process tree:",
            "    no live owned process entries are visible",
            "  known Cargo lock ownership and waiters:",
            "    no locks on mapped Cargo package-cache or target files",
        )
    ), "the renderer must preserve report text while using the captured values"


def test_stall_report_redacts_command_environment_and_process_credentials() -> None:
    """Captured diagnostics retain safe command identity without credential values."""
    environment = {
        "AWS_SESSION_TOKEN": "environment-secret-value",
        "CARGO": "https://user:password@cache.example:8443/private?token=url-secret",
        "RUSTC_WRAPPER": "sccache",
    }
    process = ProcessInfo(
        pid=123,
        parent_pid=100,
        process_group=123,
        start_time=456,
        state="S",
        command=(
            "rustc --token process-secret-value "
            "--message environment-secret-value --crate-name compile_contract"
        ),
    )
    snapshot = ProcessTreeSnapshot(
        root_pid=process.pid,
        proc_available=True,
        processes={process.pid: process},
        owned=(OwnedProcess(process, 99.0),),
    )

    report = format_stall_report(
        StallReportContext(
            process_snapshot=snapshot,
            command=(
                "cargo",
                "test",
                "--token",
                "argument-secret-value",
                "--message",
                "environment-secret-value",
                "https://user:password@cache.example/private?token=url-secret",
            ),
            cwd=pathlib.Path("/workspace"),
            environment=environment,
            elapsed_seconds=1.0,
            timeout_seconds=30.0,
            reason="timed out",
            process_observed_at=100.0,
            toolchain_file_line=None,
            lock_records=(),
            lock_path_identities=(),
        )
    )

    assert "cargo test --token <redacted>" in report, (
        "stall output must retain safe command identity and hide option values"
    )
    assert "--message <redacted>" in report, (
        "known secret environment values must be removed from arbitrary arguments"
    )
    assert "<URL https://cache.example>" in report, (
        "credential-bearing URLs must retain only their endpoint authority"
    )
    assert "RUSTC_WRAPPER=sccache" in report, (
        "safe toolchain values must remain available for diagnosis"
    )
    for secret in (
        "environment-secret-value",
        "password",
        "url-secret",
        "argument-secret-value",
        "process-secret-value",
    ):
        assert secret not in report, "stall reports must not expose credential values"


def test_diagnostic_text_is_bounded_after_redaction() -> None:
    """Long command text keeps a bounded prefix and a visible truncation mark."""
    diagnostic = diagnostics.redact_diagnostic_text("cargo " + "--verbose " * 200)

    assert len(diagnostic) <= 1000, "one diagnostic value must stay within its bound"
    assert diagnostic.startswith("cargo --verbose"), (
        "bounded output must retain the command prefix"
    )
    assert diagnostic.endswith("... [truncated]"), (
        "bounded output must explain that text was shortened"
    )


def test_stall_report_redacts_secret_values_in_lock_paths(
    tmp_path: pathlib.Path,
) -> None:
    """Lock labels hide known secrets while holder and waiter details remain."""
    secret = "cache-path-secret"
    cargo_home = tmp_path / "cargo" / secret
    lock_file = cargo_home / ".package-cache"
    lock_file.parent.mkdir(parents=True)
    lock_file.touch()
    environment = {"CARGO_HOME": str(cargo_home), "AWS_SESSION_TOKEN": secret}
    identities = collection.lock_path_identities(environment, None)
    secret_identity, raw_label = next(iter(identities.items()))
    safe_identity = "0:0:98765"
    safe_label = "Cargo target directory (/workspace/target/.cargo-lock)"
    identities[safe_identity] = safe_label
    process = ProcessInfo(
        pid=123,
        parent_pid=100,
        process_group=123,
        start_time=456,
        state="S",
        command="cargo test",
    )
    snapshot = ProcessTreeSnapshot(
        root_pid=process.pid,
        proc_available=True,
        processes={process.pid: process},
        owned=(OwnedProcess(process, 1.0),),
    )
    records = (
        LockRecord("1", False, "POSIX", "WRITE", 41, secret_identity),
        LockRecord("2", True, "POSIX", "WRITE", process.pid, secret_identity),
        LockRecord("3", False, "POSIX", "READ", 55, safe_identity),
    )

    report = format_stall_report(
        StallReportContext(
            process_snapshot=snapshot,
            command=("cargo", "test"),
            cwd=pathlib.Path("/workspace"),
            environment=environment,
            elapsed_seconds=1.0,
            timeout_seconds=30.0,
            reason="timed out",
            process_observed_at=2.0,
            toolchain_file_line=None,
            lock_records=records,
            lock_path_identities=tuple(identities.items()),
        )
    )
    redacted_lock_path = str(lock_file).replace(secret, "<redacted>")
    redacted_lock_label = f"Cargo package cache ({redacted_lock_path})"

    assert secret in raw_label, "collection must retain the raw path identity"
    assert secret not in report, "rendered lock diagnostics must hide known secrets"
    assert (
        f"{redacted_lock_label}: holder pid=41 owner=external POSIX WRITE" in report
    ), "holder output must retain ownership details with a redacted path"
    assert f"{redacted_lock_label}: waiter pid=123 owner=owned POSIX WRITE" in report, (
        "waiter output must retain ownership details with a redacted path"
    )
    assert (
        "diagnosis: ordinary contention with an external lock holder "
        f"({redacted_lock_label})" in report
    ), "waiter diagnosis must redact its lock-path label too"
    assert safe_label in report, (
        "ordinary lock paths must remain useful when they contain no secret"
    )


def test_collection_failure_redacts_and_bounds_exception_text(
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    tmp_path: pathlib.Path,
) -> None:
    """Collection exceptions cannot bypass diagnostic redaction or bounds."""
    secret = "collection-secret-value"
    environment = {"API_TOKEN": secret}

    def fail_collection(*_args: object, **_kwargs: object) -> StallReportContext:
        raise RuntimeError(
            f"failed with {secret} "
            "https://user:password@cache.example/private?token=url-secret "
            + ("additional diagnostic text " * 100)
        )

    monkeypatch.setattr(collection, "_collect_stall_report_context", fail_collection)
    request = CommandRequest(
        command=("cargo", "test"),
        cwd=tmp_path,
        environment=environment,
        purpose="focused diagnostics test",
    )

    collection.emit_stall_diagnostics(
        ProcessTreeSnapshot(1, True, {}, ()),
        request,
        types.SimpleNamespace(started_at=0.0, timeout_seconds=30.0),
        "timed out",
    )
    report = capsys.readouterr().err.rstrip("\n")

    assert "diagnostics unavailable" in report, (
        "the failure report must retain its diagnostic context"
    )
    assert (
        secret not in report and "password" not in report and "url-secret" not in report
    ), "collection failures must not expose environment or URL credentials"
    assert "<redacted>" in report and "<URL https://cache.example>" in report, (
        "collection failures must retain safe redaction markers"
    )
    assert len(report) <= 1000, "collection failure text must obey the diagnostic bound"
    assert report.endswith("... [truncated]"), (
        "bounded collection failures must explain that text was shortened"
    )
