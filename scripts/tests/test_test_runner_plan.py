"""Plan test phases while preserving target, feature, and package scope."""

from __future__ import annotations

import pathlib

import pytest
from test_runner_models import CargoTestPlan
from test_runner_options import parse_cargo_test_options
from test_runner_plan import create_test_plan
from test_runner_fixtures import package_document, workspace_with_sibling_package


def _plan_with_package_feature_gate(
    tmp_path: pathlib.Path,
    features: dict[str, list[str]],
    arguments: tuple[str, ...] = (),
) -> CargoTestPlan:
    """Plan a compile-contract target gated by one package feature."""
    metadata = package_document(tmp_path)
    package = metadata["packages"][0]
    package["features"] = features
    compile_contract = next(
        target for target in package["targets"] if target["name"] == "compile_contract"
    )
    compile_contract["required-features"] = ["gate"]
    return create_test_plan(metadata, parse_cargo_test_options(list(arguments)))


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
    assert {target.name for target in plan.nested_targets} == {
        "cli_feature_gating",
        "compile_contract",
    }, "every registered trybuild target must have its own phase"
    assert not {"cli_feature_gating", "compile_contract"} & set(
        plan.ordinary_target_args
    ), "ordinary Cargo phases must exclude every registered trybuild target"
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
    assert not {"cli_feature_gating", "compile_contract"} & set(
        plan.ordinary_target_args
    ), "--all-targets must not pass trybuild targets to outer Cargo"
    assert {target.name for target in plan.selected_targets}.issuperset(
        {"cli_feature_gating", "compile_contract"}
    ), "--all-targets must retain registered trybuild targets for their own phase"
    assert not {target.name for target in plan.ordinary_targets} & {
        "cli_feature_gating",
        "compile_contract",
    }, "registered trybuild targets must not remain in the ordinary inventory"
    assert "--all-features" in options.common, "feature selection must be preserved"
    assert not plan.run_doctests, "--all-targets must not add default doctests"


@pytest.mark.parametrize(
    ("case", "features"),
    [
        pytest.param(
            "transitive",
            {
                "default": ["first"],
                "first": ["second"],
                "second": ["gate"],
                "gate": [],
            },
            id="transitive",
        ),
        pytest.param(
            "cycle",
            {
                "default": ["first"],
                "first": ["second"],
                "second": ["first", "gate"],
                "gate": [],
            },
            id="cycle",
        ),
    ],
)
def test_package_feature_reachability_enables_required_target(
    tmp_path: pathlib.Path,
    case: str,
    features: dict[str, list[str]],
) -> None:
    """Package-local feature reachability enables the required target."""
    plan = _plan_with_package_feature_gate(
        tmp_path,
        features,
    )

    contract = next(
        target for target in plan.selected_targets if target.name == "compile_contract"
    )

    assert "gate" in contract.enabled_features, (
        f"{case} package feature reachability must satisfy target metadata"
    )


@pytest.mark.parametrize(
    "activation",
    ["dep:gate", "dependency/gate"],
    ids=["dep-activation", "dependency-feature"],
)
def test_dependency_activations_do_not_enable_package_feature(
    tmp_path: pathlib.Path,
    activation: str,
) -> None:
    """Dependency feature activations do not satisfy package target gates."""
    plan = _plan_with_package_feature_gate(
        tmp_path,
        {
            "default": ["first"],
            "first": [activation],
            "gate": [],
        },
    )

    assert not any(
        target.name == "compile_contract" for target in plan.selected_targets
    ), f"{activation} must not satisfy a package target's required feature"


def test_requested_package_feature_enables_required_target(
    tmp_path: pathlib.Path,
) -> None:
    """An explicitly requested package-qualified feature remains enabled."""
    plan = _plan_with_package_feature_gate(
        tmp_path,
        {"default": [], "gate": []},
        ("--no-default-features", "--features", "podbot/gate"),
    )
    contract = next(
        target for target in plan.selected_targets if target.name == "compile_contract"
    )

    assert "gate" in contract.enabled_features, (
        "an explicitly requested package feature must satisfy the target gate"
    )


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

    for nested_target in ("cli_feature_gating", "compile_contract"):
        assert nested_target not in phases["podbot"], (
            "Podbot's registered nested targets must be excluded from its Cargo phase"
        )
        assert nested_target in phases["sibling"], (
            "same-named sibling targets must remain in their owning package phase"
        )


def test_no_default_cli_trybuild_selection_has_no_outer_cargo_phase(
    tmp_path: pathlib.Path,
) -> None:
    """The no-default CLI boundary harness runs after its parent Cargo exits."""
    options = parse_cargo_test_options(
        ["--no-default-features", "--test", "cli_feature_gating"]
    )

    plan = create_test_plan(package_document(tmp_path), options)

    assert plan.ordinary_package_args == (), (
        "the no-default CLI trybuild target must not run under outer Cargo"
    )
    assert [target.name for target in plan.nested_targets] == ["cli_feature_gating"], (
        "the no-default CLI boundary target must use isolated execution"
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
