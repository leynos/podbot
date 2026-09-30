"""Run ordinary, documentation, and nested-Cargo test phases."""

from __future__ import annotations

from collections.abc import Callable, Iterable

from test_runner_commands import (
    first_failure,
    print_skipped_phases,
    without_package_selection,
)
from test_runner_context import TestRunnerContext
from test_runner_models import CargoTestOptions, CargoTestPlan
from test_runner_nested import run_nested_target
from test_runner_supervisor import CommandRequest


def run_test_plan(context: TestRunnerContext, plan: CargoTestPlan) -> int:
    """Run the planned test phases while preserving Cargo fail-fast policy.

    Examples
    --------
    A compile-contract executable starts only after its Cargo build returns.
    """
    if plan.options.no_run:
        return _run_no_run(context, plan)

    ordinary_results, should_stop = _run_ordinary_phase(context, plan)
    statuses = list(ordinary_results)
    if context.supervisor.terminal_status is not None:
        return context.supervisor.terminal_status
    if should_stop:
        print_skipped_phases(
            plan,
            ordinary_offset=len(ordinary_results),
            include_doctests=True,
            include_nested=True,
        )
        return first_failure(statuses)

    doctest_results, should_stop = _run_doctest_phase(context, plan)
    statuses.extend(doctest_results)
    if context.supervisor.terminal_status is not None:
        return context.supervisor.terminal_status
    if should_stop:
        print_skipped_phases(
            plan,
            ordinary_offset=len(plan.ordinary_package_args),
            include_nested=True,
        )
        return first_failure(statuses)

    nested_results, _ = _run_nested_phase(context, plan)
    statuses.extend(nested_results)
    if context.supervisor.terminal_status is not None:
        return context.supervisor.terminal_status
    return first_failure(statuses)


def _run_ordinary_phase(
    context: TestRunnerContext,
    plan: CargoTestPlan,
) -> tuple[tuple[int, ...], bool]:
    """Run ordinary package tests and report whether fail-fast stopped work."""
    if not plan.ordinary_package_args:
        print("== No ordinary test targets selected; skipping phase ==", flush=True)
        return (), False

    def run_package(
        selection: tuple[str, tuple[str, ...]],
    ) -> int:
        package_name, target_arguments = selection
        print(
            f"== Running ordinary Cargo test targets for {package_name} ==",
            flush=True,
        )
        return _run_cargo_test(
            context,
            plan,
            target_arguments,
            package_name=package_name,
        )

    return _run_fail_fast_items(context, plan, plan.ordinary_package_args, run_package)


def _run_doctest_phase(
    context: TestRunnerContext,
    plan: CargoTestPlan,
) -> tuple[tuple[int, ...], bool]:
    """Run documentation tests and report whether fail-fast stopped work."""
    if not plan.run_doctests:
        print("== No documentation tests selected; skipping phase ==", flush=True)
        return (), False
    print("== Running Cargo documentation tests ==", flush=True)
    status = _run_cargo_test(
        context,
        plan,
        ("--doc",),
    )
    should_stop = bool(status and not plan.options.no_fail_fast)
    return (status,), should_stop


def _run_nested_phase(
    context: TestRunnerContext,
    plan: CargoTestPlan,
) -> tuple[tuple[int, ...], bool]:
    """Run nested-Cargo tests and report whether fail-fast stopped work."""
    if not plan.nested_targets:
        print("== No nested-Cargo targets selected; skipping phase ==", flush=True)
        return (), False
    return _run_fail_fast_items(
        context,
        plan,
        plan.nested_targets,
        lambda target: run_nested_target(context, plan, target),
    )


def _run_fail_fast_items[PhaseItem](
    context: TestRunnerContext,
    plan: CargoTestPlan,
    items: Iterable[PhaseItem],
    run_item: Callable[[PhaseItem], int],
) -> tuple[tuple[int, ...], bool]:
    """Run an ordered target set under Cargo's selected fail-fast policy.

    This loop is shared only by ordinary package selections and registered
    nested-Cargo targets; doctests remain a single Cargo phase.
    """
    statuses: list[int] = []
    for item in items:
        status = run_item(item)
        statuses.append(status)
        if context.supervisor.terminal_status is not None:
            return tuple(statuses), True
        if status and not plan.options.no_fail_fast:
            return tuple(statuses), True
    return tuple(statuses), False


def _run_no_run(context: TestRunnerContext, plan: CargoTestPlan) -> int:
    """Compile selected tests without launching test harnesses."""
    print("== Compiling selected tests without execution ==", flush=True)
    command = [*context.cargo_command, "test", *plan.options.common]
    command.extend(plan.selected_target_args)
    _append_test_filter(command, plan.options)
    command.append("--no-run")
    _append_harness_flags(command, plan.options)
    return context.supervisor.run_inherited(
        CommandRequest(
            command, plan.workspace_root, context.environment, "Cargo test --no-run"
        )
    )


def _run_cargo_test(
    context: TestRunnerContext,
    plan: CargoTestPlan,
    target_arguments: tuple[str, ...],
    *,
    package_name: str | None = None,
) -> int:
    """Run one ordinary Cargo test phase with caller filters and flags."""
    options = plan.options
    common = (
        without_package_selection(options.common)
        if package_name is not None
        else options.common
    )
    package_arguments = ["--package", package_name] if package_name else []
    command = [
        *context.cargo_command,
        "test",
        *common,
        *package_arguments,
        *target_arguments,
    ]
    _append_test_filter(command, options)
    _append_harness_flags(command, options)
    return context.supervisor.run_inherited(
        CommandRequest(
            command, plan.workspace_root, context.environment, "ordinary Cargo tests"
        )
    )


def _append_test_filter(command: list[str], options: CargoTestOptions) -> None:
    """Append Cargo's positional test filter when one was selected."""
    if options.test_filter:
        command.append(options.test_filter)


def _append_harness_flags(command: list[str], options: CargoTestOptions) -> None:
    """Append libtest arguments after Cargo's separator when present."""
    if options.harness_args:
        command.extend(("--", *options.harness_args))
