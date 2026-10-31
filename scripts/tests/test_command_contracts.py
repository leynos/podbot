"""The contracts must be run by CI, by a step nothing can skip.

A contract nothing runs is a comment. These cases pin down how a step
running a command is recognized, and which guards disqualify it.
"""

from __future__ import annotations

import os
import platform
import re
import subprocess
import tomllib
from pathlib import Path
import typing as typ

import pytest
from workflow_commands import command_steps
from workflow_contracts import of_type, parse as parse_workflow

#: The command CI must run to execute the contracts in this directory.
CONTRACT_COMMAND: typ.Final[str] = "make workflow-contracts"
TEST_COMMANDS: typ.Final[tuple[str, ...]] = (
    "make test TEST_FLAGS='--no-default-features --test cli_feature_gating --test compile_contract'",
    "make test TEST_FLAGS='--features internal'",
)


MOLD_FLAG: typ.Final[str] = "-Clink-arg=-fuse-ld=mold"
LINUX_TABLE: typ.Final[str] = 'cfg(target_os = "linux")'


def _dry_run_test_recipe(
    environment: dict[str, str],
    overrides: tuple[str, ...] = (),
    target: str = "test",
) -> str:
    """Return normalized output from a dry-run of a Make gate target."""
    repository_root = Path(__file__).resolve().parents[2]
    output = subprocess.run(
        ["make", "--no-print-directory", "--dry-run", target, *overrides],
        cwd=repository_root,
        env=environment,
        capture_output=True,
        check=True,
        text=True,
    ).stdout
    return " ".join(output.replace("\\\n\t", " ").split())


def test_ci_runs_each_test_lane_through_make(
    workflow_texts: dict[str, str],
) -> None:
    """The feature-specific lanes use the supervised Make entry point."""
    document = parse_workflow("ci.yml", workflow_texts["ci.yml"])

    for command in TEST_COMMANDS:
        running = command_steps(document, command)
        assert len(running) == 1, (
            f"exactly one step in ci.yml must run exactly {command!r}; "
            f"{len(running)} do"
        )
        assert running[0].can_run, (
            f"the step running {command!r} is guarded by {running[0].describe_guards()}"
        )


def test_ci_does_not_run_cargo_test_directly(
    workflow_texts: dict[str, str],
) -> None:
    """No workflow run block can bypass nested-Cargo supervision."""
    document = parse_workflow("ci.yml", workflow_texts["ci.yml"])
    run_blocks = (
        str(step_map.get("run", ""))
        for job in of_type(document.get("jobs"), dict).values()
        for step in of_type(of_type(job, dict).get("steps"), list)
        for step_map in (of_type(step, dict),)
    )
    direct_tests = [
        block for block in run_blocks if re.search(r"\bcargo\s+test\b", block)
    ]

    assert not direct_tests, (
        "CI run blocks must use `make test` so nested-Cargo targets run only "
        f"after Cargo exits: {direct_tests!r}"
    )


def test_make_test_uses_runner_and_preserves_overrides(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The recipe keeps Cargo flags, warning policy, and timeout overridable."""
    inherited_make_variables = {
        "MAKEFLAGS": "-j 11",
        "MFLAGS": "-j 13",
        "MAKEOVERRIDES": "TEST_FLAGS=--doc",
        "BUILD_JOBS": "-j 7",
        "RUST_FLAGS": "-W unused",
        "CARGO_FLAGS": "--doc",
        "TEST_FLAGS": "--doc",
        "TEST_TIMEOUT": "9",
        "PODBOT_TEST_TIMEOUT": "11",
        "CARGO": "cargo-from-environment",
        "UV": "uv-from-environment",
    }
    for name, value in inherited_make_variables.items():
        monkeypatch.setenv(name, value)
    monkeypatch.setenv("UNRELATED_TEST_ENV", "preserve-me")
    make_environment = os.environ.copy()
    for name in inherited_make_variables:
        make_environment.pop(name, None)

    default_command = _dry_run_test_recipe(make_environment)
    override_command = _dry_run_test_recipe(
        make_environment,
        (
            "TEST_FLAGS=--no-default-features --test cli_feature_gating -- --nocapture",
            "TEST_TIMEOUT=42",
            "RUST_FLAGS=-D warnings -W unused",
            "BUILD_JOBS=-j 2",
        ),
    )

    assert "uv run --no-project --python 3.14" in default_command, (
        "the test recipe must use the configured Python runtime"
    )
    assert "python scripts/test_runner.py" in default_command, (
        "the test recipe must invoke the bounded test runner"
    )
    assert '--timeout "1800" -- --all-targets --all-features' in default_command, (
        "the default recipe must preserve the full target and feature selection"
    )
    assert 'RUSTFLAGS="-D warnings' in default_command, (
        "the default recipe must deny compiler warnings"
    )
    assert ("-Clink-arg=-fuse-ld=mold" in default_command) == (
        platform.system() == "Linux"
    ), "the default recipe must restate the mold linker flag on Linux only"
    assert (
        '--timeout "42" -- -j 2 --no-default-features --test cli_feature_gating'
        in override_command
    ), "the test recipe must preserve timeout, job, and test-flag overrides"
    assert "-- --nocapture" in override_command, (
        "the test recipe must forward harness arguments"
    )
    assert 'RUSTFLAGS="-D warnings -W unused' in override_command, (
        "the test recipe must preserve the caller's Rust flags"
    )
    assert make_environment["UNRELATED_TEST_ENV"] == "preserve-me", (
        "dry-run isolation must preserve unrelated caller environment values"
    )


@pytest.mark.parametrize(
    ("timeout_environment", "expected_timeout"),
    (
        pytest.param({"PODBOT_TEST_TIMEOUT": "11"}, "11", id="fallback"),
        pytest.param(
            {"PODBOT_TEST_TIMEOUT": "11", "TEST_TIMEOUT": "42"},
            "42",
            id="explicit-test-timeout-wins",
        ),
    ),
)
def test_make_test_resolves_timeout_environment(
    timeout_environment: dict[str, str],
    expected_timeout: str,
) -> None:
    """The environment fallback applies unless TEST_TIMEOUT is set."""
    make_environment = os.environ.copy()
    make_environment.pop("TEST_TIMEOUT", None)
    make_environment.pop("PODBOT_TEST_TIMEOUT", None)
    make_environment.update(timeout_environment)

    command = _dry_run_test_recipe(make_environment)

    assert f'--timeout "{expected_timeout}" --' in command, (
        "the runner must receive the resolved test timeout"
    )


@pytest.fixture(scope="module")
def _workflow_gate_commands() -> tuple[str, ...]:
    """Return the normalized commands Make would run for workflow-contracts."""
    repository_root = Path(__file__).resolve().parents[2]
    output = subprocess.run(
        ["make", "--no-print-directory", "--dry-run", "workflow-contracts"],
        cwd=repository_root,
        capture_output=True,
        check=True,
        text=True,
    ).stdout
    return tuple(output.replace("\\\n\t", " ").splitlines())


@pytest.fixture(scope="module")
def _runner_modules() -> tuple[str, ...]:
    """Discover sorted runner-module paths relative to the repository root."""
    repository_root = Path(__file__).resolve().parents[2]
    return tuple(
        path.relative_to(repository_root).as_posix()
        for path in sorted((repository_root / "scripts").glob("test_runner*.py"))
    )


@pytest.mark.parametrize(
    ("command_terms", "missing_from"),
    [
        pytest.param(("ruff", "format"), "Ruff formatting", id="format"),
        pytest.param(("ruff", "check"), "Ruff linting", id="lint"),
        pytest.param(("python -m pytest",), "doctests", id="doctests"),
    ],
)
def test_make_workflow_gate_formats_lints_and_documents_runner_modules(
    command_terms: tuple[str, ...],
    missing_from: str,
    _workflow_gate_commands: tuple[str, ...],
    _runner_modules: tuple[str, ...],
) -> None:
    """Every runner module stays covered by its selected Python gate."""
    selected_command = next(
        (
            command
            for command in _workflow_gate_commands
            if all(term in command for term in command_terms)
        ),
        None,
    )
    assert selected_command is not None, (
        f"workflow-contracts is missing a command containing {command_terms!r}"
    )
    assert _runner_modules, "the Python test runner must have modules to validate"
    for module in _runner_modules:
        assert module in selected_command, f"{module} is missing from {missing_from}"


@pytest.mark.parametrize(
    "module",
    [
        "scripts/check_sccache_health.py",
        "scripts/report_sccache_errors.py",
    ],
    ids=["sccache-health", "sccache-diagnostics"],
)
def test_workflow_gate_includes_sccache_doctests(
    module: str,
    _workflow_gate_commands: tuple[str, ...],
) -> None:
    """Both cache helpers remain covered by the workflow doctest command."""
    pytest_command = next(
        (
            command
            for command in _workflow_gate_commands
            if "python -m pytest" in command
        ),
        None,
    )
    assert pytest_command is not None, "workflow-contracts must run Python doctests"
    assert module in pytest_command, f"{module} is missing from doctests"


def test_the_contracts_are_run_by_ci(workflow_texts: dict[str, str]) -> None:
    """A contract nothing runs is a comment.

    The assertion is on the command rather than on a step named
    "Workflow contracts": a step can be renamed, and a step whose `run:`
    was changed to something else would keep the name and stop asserting
    anything. Equality on the stripped value, not a substring search, since
    a search is satisfied by `echo 'make workflow-contracts'`.

    Both guards are checked. A step with no `if:` inside a job with
    `if: false` is dead code, and a contract reading only the step's own
    attributes stays green while the command never runs.
    """
    document = parse_workflow("ci.yml", workflow_texts["ci.yml"])
    running = command_steps(document, CONTRACT_COMMAND)

    assert len(running) == 1, (
        f"exactly one step in ci.yml must run exactly {CONTRACT_COMMAND!r}; "
        f"{len(running)} do. A step whose run: merely contains that text, "
        f"such as an echo or a comment, does not count"
    )
    assert running[0].can_run, (
        f"the step running {CONTRACT_COMMAND!r} in job {running[0].job!r} is "
        f"guarded by {running[0].describe_guards()}, so it can be skipped "
        "without failing anything"
    )


@pytest.mark.parametrize(
    ("job_guard", "step_guard", "can_run"),
    [
        ("", "", True),
        ("    if: false\n", "", False),
        ("", "        if: false\n", False),
        ("    if: ${{ github.event_name == 'push' }}\n", "", False),
        ("", "        if: ${{ false }}\n", False),
    ],
    ids=["unguarded", "job-guard", "step-guard", "job-expression", "step-expression"],
)
def test_a_guard_at_either_scope_stops_the_command_running(
    job_guard: str, step_guard: str, can_run: bool
) -> None:
    """Driven over constructed workflows, because the real one is unguarded.

    The contract above is parametrized over a file that has no `if:`
    anywhere, so it passes whether or not the rule reads the job. Only a
    constructed guarded job shows the job scope being read at all.
    """
    text = (
        "jobs:\n  lint:\n"
        f"{job_guard}"
        "    steps:\n"
        f"      - run: {CONTRACT_COMMAND}\n"
        f"{step_guard}"
    )
    found = command_steps(parse_workflow("ci.yml", text), CONTRACT_COMMAND)

    assert len(found) == 1
    assert found[0].job == "lint"
    assert found[0].can_run is can_run


def test_a_mentioned_command_is_not_a_running_one() -> None:
    """Equality, shown to reject the spellings a search accepts."""
    text = (
        "jobs:\n  lint:\n    steps:\n"
        f"      - run: echo '{CONTRACT_COMMAND}'\n"
        f"      - run: '# {CONTRACT_COMMAND}'\n"
        f"      - run: {CONTRACT_COMMAND} --dry-run\n"
    )
    assert command_steps(parse_workflow("ci.yml", text), CONTRACT_COMMAND) == ()


def test_two_contract_command_steps_are_both_reported() -> None:
    """The contract asserts exactly one, so the reader must find both."""
    text = (
        "jobs:\n  lint:\n    steps:\n"
        f"      - run: {CONTRACT_COMMAND}\n"
        "  check:\n    steps:\n"
        f"      - run: {CONTRACT_COMMAND}\n"
    )
    found = command_steps(parse_workflow("ci.yml", text), CONTRACT_COMMAND)

    assert [step.job for step in found] == ["lint", "check"]


@pytest.mark.parametrize("target", ["test", "typecheck"])
@pytest.mark.parametrize(
    ("host_os", "expects_mold"),
    [("Linux", True), ("Darwin", False), ("Windows_NT", False)],
)
def test_gate_recipes_restate_mold_on_linux_only(
    target: str, host_os: str, expects_mold: bool
) -> None:
    """An assigned RUSTFLAGS replaces the configuration, so the gates add mold.

    The host is controlled through `BUILD_HOST_OS`, so both branches run on any
    machine, and the caller's `RUST_FLAGS` survive in either branch.
    """
    command = _dry_run_test_recipe(
        os.environ.copy(),
        (f"BUILD_HOST_OS={host_os}", "RUST_FLAGS=-D warnings -W unused"),
        target,
    )

    assert 'RUSTFLAGS="-D warnings -W unused' in command, (
        f"make {target} must preserve the caller's Rust flags"
    )
    assert (MOLD_FLAG in command) is expects_mold, (
        f"make {target} on {host_os} must {'add' if expects_mold else 'omit'} mold"
    )


def test_cargo_configuration_scopes_mold_to_linux() -> None:
    """The linker flag lives in the Linux table and in no platform-wide source."""
    root = Path(__file__).resolve().parents[2]
    config = tomllib.loads((root / ".cargo" / "config.toml").read_text("utf-8"))

    assert MOLD_FLAG in config["target"][LINUX_TABLE]["rustflags"], (
        "the Linux target table must select mold"
    )
    assert MOLD_FLAG not in config.get("build", {}).get("rustflags", []), (
        "mold ships for Linux only, so [build] must not name it"
    )
    assert not config.get("profile"), (
        "the stable pin cannot select a codegen backend, so no profile may"
    )


def test_ci_setup_rust_installs_mold_and_restates_the_linker(
    workflow_texts: dict[str, str],
) -> None:
    """The hosted gate installs mold and keeps it beside the warning policy.

    `setup-rust` exports its `rustflags` input as `RUSTFLAGS`, which replaces
    `.cargo/config.toml`, so the input must carry the linker flag next to
    `-D warnings` or `make lint`'s cargo doc and Clippy link without mold.
    """
    document = parse_workflow("ci.yml", workflow_texts["ci.yml"])
    setup_steps = [
        step_map
        for job in of_type(document.get("jobs"), dict).values()
        for step in of_type(of_type(job, dict).get("steps"), list)
        for step_map in (of_type(step, dict),)
        if "/setup-rust@" in str(step_map.get("uses", ""))
    ]
    assert len(setup_steps) == 1, "ci.yml must run exactly one setup-rust step"

    inputs = of_type(setup_steps[0].get("with"), dict)
    assert inputs.get("install-mold") == "true", (
        "the setup-rust step must install the mold linker the configuration selects"
    )
    rustflags = str(inputs.get("rustflags"))
    assert "-D warnings" in rustflags, "setup-rust must keep denying warnings"
    assert MOLD_FLAG in rustflags, "setup-rust must restate the mold linker flag"
