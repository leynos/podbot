"""Build and execute registered test harnesses that launch nested Cargo."""

from __future__ import annotations

import sys
import typing as typ

from test_runner_cargo import (
    create_test_runtime_environment,
    parse_cargo_json_message,
    select_test_executables,
)
from test_runner_commands import without_message_format, without_package_selection
from test_runner_context import TestRunnerContext
from test_runner_models import CargoTestPlan, RunnerError, Target
from test_runner_supervisor import CommandRequest


def run_nested_target(
    context: TestRunnerContext,
    plan: CargoTestPlan,
    target: Target,
) -> int:
    """Build one registered test target, then invoke its current artifact."""
    key = (target.package_id, target.name)
    status, messages = _build_nested_target(context, plan, target)
    if status != 0:
        return status
    try:
        executable = select_test_executables(messages, (target,))[key]
    except RunnerError as exc:
        print(f"test runner: {exc}", file=sys.stderr)
        return 2
    package = _selected_target_package(plan, target)
    if package is None:
        print(
            "test runner: selected package "
            f"{target.package_name} is missing from the test plan",
            file=sys.stderr,
        )
        return 2
    environment = create_test_runtime_environment(
        context, package, executable, messages
    )
    arguments = [plan.options.test_filter] if plan.options.test_filter else []
    arguments.extend(plan.options.harness_args)
    print(
        f"== Running {target.package_name}:{target.name} after Cargo exited ==",
        flush=True,
    )
    return context.supervisor.run_inherited(
        CommandRequest(
            [str(executable), *arguments],
            target.manifest_dir,
            environment,
            f"{target.package_name}:{target.name} test harness",
        )
    )


def _build_nested_target(
    context: TestRunnerContext,
    plan: CargoTestPlan,
    target: Target,
) -> tuple[int, list[dict[str, typ.Any]]]:
    """Run and reap Cargo's JSON build before returning artifact messages."""
    print(
        f"== Building nested-Cargo target {target.package_name}:{target.name} ==",
        flush=True,
    )
    messages: list[dict[str, typ.Any]] = []
    status = _run_json_build(
        context,
        _nested_build_command(context, plan, target),
        messages,
    )
    print(f"Cargo no-run build exited with status {status}.", flush=True)
    return status, messages


def _nested_build_command(
    context: TestRunnerContext,
    plan: CargoTestPlan,
    target: Target,
) -> list[str]:
    """Select exactly one registered target and force structured build output."""
    return [
        *context.cargo_command,
        "test",
        *without_message_format(without_package_selection(plan.options.common)),
        "--package",
        target.package_name,
        *target.cargo_selector(),
        "--no-run",
        "--message-format=json-render-diagnostics",
    ]


def _selected_target_package(
    plan: CargoTestPlan,
    target: Target,
) -> dict[str, typ.Any] | None:
    """Find the metadata package that owns this selected target."""
    return next(
        (
            package
            for package in plan.selected_packages
            if package.get("id") == target.package_id
        ),
        None,
    )


def _run_json_build(
    context: TestRunnerContext,
    command: list[str],
    messages: list[dict[str, typ.Any]],
) -> int:
    """Stream a JSON-mode Cargo build and retain its current artifacts."""

    def emit_line(line: str) -> None:
        message = parse_cargo_json_message(line)
        if message is None:
            sys.stdout.write(line)
            sys.stdout.flush()
            return
        messages.append(message)
        _write_rendered_diagnostic(message)

    return context.supervisor.run_lines(
        CommandRequest(
            command,
            context.cwd,
            context.environment,
            "Cargo nested-target build",
        ),
        emit_line,
    )


def _write_rendered_diagnostic(message: dict[str, typ.Any]) -> None:
    """Write Cargo's human-readable diagnostic without echoing its JSON."""
    diagnostic = message.get("message")
    if not isinstance(diagnostic, dict):
        return
    rendered = diagnostic.get("rendered")
    if isinstance(rendered, str):
        sys.stderr.write(rendered)
        sys.stderr.flush()
