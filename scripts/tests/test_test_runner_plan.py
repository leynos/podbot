"""Specify target selection and Cargo option preservation for the runner."""

from __future__ import annotations

import pathlib

import pytest
from test_runner_models import RunnerError
from test_runner_options import parse_cargo_test_options
from test_runner_plan import create_test_plan
from test_runner_selection import validate_nested_registry

from test_runner_fixtures import package_document, workspace_with_sibling_package


def test_default_targets_keep_doctests_and_remove_nested_target(
    tmp_path: pathlib.Path,
) -> None:
    """Default test runs keep doctests while Cargo excludes trybuild target."""
    metadata = package_document(tmp_path)
    options = parse_cargo_test_options([])

    plan = create_test_plan(metadata, options)

    assert plan.run_doctests, "default Cargo tests must retain library doctests"
    assert not any(target.name == "benchmarks" for target in plan.selected_targets), (
        "default Cargo tests must not select benchmark targets"
    )
    assert [target.name for target in plan.nested_targets] == ["compile_contract"], (
        "the registered nested-Cargo target must have its own phase"
    )
    assert "--test" not in plan.ordinary_target_args or all(
        plan.ordinary_target_args[index + 1] != "compile_contract"
        for index, value in enumerate(plan.ordinary_target_args[:-1])
        if value == "--test"
    ), "ordinary Cargo phases must exclude the registered nested test"
    assert "--doc" not in plan.ordinary_target_args, (
        "doctests must remain in their dedicated Cargo phase"
    )


def test_all_targets_expands_categories_and_excludes_registered_test(
    tmp_path: pathlib.Path,
) -> None:
    """The all-target inventory remains complete except for nested tests."""
    options = parse_cargo_test_options(["--all-targets", "--all-features"])
    plan = create_test_plan(package_document(tmp_path), options)

    assert {"--lib", "--bins", "--examples", "--benches"}.issubset(
        set(plan.ordinary_target_args)
    ), "--all-targets must retain each ordinary target category"
    assert "compile_contract" not in plan.ordinary_target_args, (
        "--all-targets must not pass the nested test to an outer Cargo process"
    )
    assert "cli_feature_gating" in plan.ordinary_target_args, (
        "ordinary integration tests must remain selected"
    )
    assert "--all-features" in options.common, "feature selection must be preserved"
    assert not plan.run_doctests, "--all-targets must not add default doctests"


def test_doc_only_selection_has_one_documentation_phase(
    tmp_path: pathlib.Path,
) -> None:
    """A doc-only request does not duplicate doctests in ordinary phases."""
    plan = create_test_plan(
        package_document(tmp_path), parse_cargo_test_options(["--doc"])
    )

    assert plan.ordinary_package_args == (), (
        "--doc must not schedule an ordinary Cargo test phase"
    )
    assert plan.run_doctests, "--doc must retain its dedicated documentation phase"


def test_all_targets_omits_benches_when_package_has_no_bench_harness(
    tmp_path: pathlib.Path,
) -> None:
    """The bench selector is emitted only when metadata enables a bench harness."""
    metadata = package_document(tmp_path)
    metadata["packages"][0]["targets"] = [
        target
        for target in metadata["packages"][0]["targets"]
        if "bench" not in target["kind"]
    ]
    for target in metadata["packages"][0]["targets"]:
        target["bench"] = False

    plan = create_test_plan(metadata, parse_cargo_test_options(["--all-targets"]))

    assert "--benches" not in plan.ordinary_target_args, (
        "Cargo selectors must not request a bench target absent from metadata"
    )


def test_relative_target_directory_is_anchored_to_workspace_root(
    tmp_path: pathlib.Path,
) -> None:
    """A metadata-relative target directory is based at the workspace root."""
    metadata = package_document(tmp_path)
    workspace_root = tmp_path / "workspace"
    metadata["workspace_root"] = str(workspace_root)
    metadata["target_directory"] = "target-build"

    plan = create_test_plan(metadata, parse_cargo_test_options([]))

    assert plan.target_directory == workspace_root / "target-build", (
        "metadata-relative target paths must not depend on the runner's cwd"
    )


def test_workspace_phases_scope_same_named_targets_by_package(
    tmp_path: pathlib.Path,
) -> None:
    """A sibling target cannot reselect Podbot's registered nested target."""
    metadata = workspace_with_sibling_package(tmp_path)
    options = parse_cargo_test_options(["--workspace", "--all-targets"])

    plan = create_test_plan(metadata, options)
    phases = dict(plan.ordinary_package_args)

    assert "compile_contract" not in phases["podbot"], (
        "Podbot's registered nested target must be excluded from its Cargo phase"
    )
    assert "compile_contract" in phases["sibling"], (
        "a same-named sibling target must remain in its owning package phase"
    )


def test_specific_compile_contract_selection_preserves_features_and_filters(
    tmp_path: pathlib.Path,
) -> None:
    """Feature flags and harness arguments survive the direct-execution split."""
    options = parse_cargo_test_options(
        [
            "--no-default-features",
            "--features",
            "internal",
            "--jobs",
            "3",
            "--test",
            "compile_contract",
            "stable_exec_context_signatures_compile",
            "--",
            "--exact",
            "--nocapture",
        ]
    )
    plan = create_test_plan(package_document(tmp_path), options)

    assert plan.ordinary_target_args == (), (
        "selecting only the nested target must leave no ordinary test phase"
    )
    assert [target.name for target in plan.nested_targets] == ["compile_contract"], (
        "the explicit target selection must reach the nested phase"
    )
    assert options.common == (
        "--no-default-features",
        "--features",
        "internal",
        "--jobs",
        "3",
    ), "feature and job options must remain common to all Cargo phases"
    assert options.test_filter == "stable_exec_context_signatures_compile", (
        "the positional test filter must be preserved"
    )
    assert options.harness_args == ("--exact", "--nocapture"), (
        "libtest arguments must remain after the Cargo separator"
    )
    assert not plan.run_doctests, "explicit integration-test selection omits doctests"


@pytest.mark.parametrize(
    "arguments",
    [
        ["--config", "build.target=x86_64-unknown-linux-gnu"],
        ["--doc", "--test", "compile_contract"],
        ["--exclude", "podbot"],
        ["--target-dir"],
        ["one", "two"],
    ],
    ids=[
        "unsafe-config",
        "doc-and-test",
        "exclude-needs-workspace",
        "missing-option-value",
        "two-filters",
    ],
)
def test_unsupported_mappings_fail_before_running_cargo(arguments: list[str]) -> None:
    """Options the runner cannot map consistently fail at the CLI boundary."""
    with pytest.raises(RunnerError):
        parse_cargo_test_options(arguments)


def test_option_values_may_start_with_a_hyphen() -> None:
    """Required values are consumed even when they resemble options."""
    options = parse_cargo_test_options(["--target-dir", "-build"])

    assert options.target_dir == pathlib.Path.cwd().resolve() / "-build", (
        "a leading hyphen in a value must not be mistaken for a missing value"
    )


def test_registry_rejects_a_removed_target(tmp_path: pathlib.Path) -> None:
    """The registry cannot silently outlive a renamed or removed test target."""
    metadata = package_document(tmp_path)
    metadata["packages"][0]["targets"] = [
        target
        for target in metadata["packages"][0]["targets"]
        if target["name"] != "compile_contract"
    ]

    with pytest.raises(RunnerError, match="registered nested-Cargo target"):
        validate_nested_registry(metadata)


def test_registry_ignores_packages_outside_the_workspace(
    tmp_path: pathlib.Path,
) -> None:
    """A repository-specific registry does not constrain another workspace."""
    metadata = package_document(tmp_path)
    metadata["workspace_members"] = []

    validate_nested_registry(metadata)
