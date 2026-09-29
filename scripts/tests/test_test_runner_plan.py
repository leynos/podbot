"""Specify target selection and Cargo option preservation for the runner."""

from __future__ import annotations

import pathlib

import pytest
from test_runner_models import CargoTestPlan, RunnerError
from test_runner_options import parse_cargo_test_options
from test_runner_plan import create_test_plan
from test_runner_selection import validate_nested_registry

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
    ("arguments", "cli_enabled"),
    [
        ([], True),
        (["--no-default-features"], False),
        (["--no-default-features", "--features", "internal"], False),
        (["--no-default-features", "--features", "cli"], True),
    ],
    ids=["package-default", "no-defaults", "other-feature", "explicit-cli"],
)
def test_default_target_selection_respects_required_features(
    tmp_path: pathlib.Path,
    arguments: list[str],
    cli_enabled: bool,
) -> None:
    """Default feature state controls inclusion of the CLI binary target."""
    options = parse_cargo_test_options(arguments)

    plan = create_test_plan(package_document(tmp_path), options)

    selected_cli_binary = any(
        target.name == "podbot" and "bin" in target.kinds
        for target in plan.selected_targets
    )
    target_arguments = tuple(
        zip(plan.ordinary_target_args, plan.ordinary_target_args[1:])
    )
    assert selected_cli_binary is cli_enabled, (
        "default selection must include the binary only when its required cli "
        "feature is active"
    )
    assert (("--bin", "podbot") in target_arguments) is cli_enabled, (
        "ordinary Cargo arguments must match enabled target features"
    )


def test_plural_test_selection_skips_feature_gated_binary(
    tmp_path: pathlib.Path,
) -> None:
    """Plural test selection omits targets excluded by Cargo feature gates."""
    options = parse_cargo_test_options(["--no-default-features", "--tests"])

    plan = create_test_plan(package_document(tmp_path), options)

    selected_cli_binary = any(
        target.name == "podbot" and "bin" in target.kinds
        for target in plan.selected_targets
    )
    assert not selected_cli_binary, (
        "the cli-gated binary must be absent from no-default test selection"
    )
    assert "--bins" not in plan.ordinary_target_args, (
        "the ordinary Cargo phase must not select a disabled binary group"
    )


def test_plural_bin_selection_returns_empty_when_every_match_is_feature_gated(
    tmp_path: pathlib.Path,
) -> None:
    """Plural selectors retain no targets when every match fails its gate."""
    plan = create_test_plan(
        package_document(tmp_path),
        parse_cargo_test_options(["--no-default-features", "--bins"]),
    )

    assert plan.selected_targets == (), (
        "a plural selector with only feature-gated matches must return no targets"
    )


def test_plural_selector_without_matches_raises_runner_error(
    tmp_path: pathlib.Path,
) -> None:
    """A plural category with no metadata matches is an invalid selection."""
    metadata = package_document(tmp_path)
    package = metadata["packages"][0]
    package["targets"] = [
        target for target in package["targets"] if "bench" not in target["kind"]
    ]
    for target in package["targets"]:
        target["bench"] = False

    with pytest.raises(RunnerError, match="Cargo target selection contains no targets"):
        create_test_plan(metadata, parse_cargo_test_options(["--benches"]))


def test_repeated_and_mixed_selectors_preserve_target_order_and_deduplicate(
    tmp_path: pathlib.Path,
) -> None:
    """Repeated selectors preserve first-match order and target identity."""
    plan = create_test_plan(
        package_document(tmp_path),
        parse_cargo_test_options(
            [
                "--test",
                "compile_contract",
                "--example",
                "example_check",
                "--tests",
                "--example",
                "example_check",
            ]
        ),
    )

    assert [(target.name, target.kinds) for target in plan.selected_targets] == [
        ("compile_contract", ("test",)),
        ("example_check", ("example",)),
        ("podbot", ("lib",)),
        ("podbot", ("bin",)),
        ("cli_feature_gating", ("test",)),
    ], "selection must retain first occurrence order and deduplicate by target identity"


def test_transitive_package_features_enable_required_target(
    tmp_path: pathlib.Path,
) -> None:
    """A package-local feature chain enables its final target gate."""
    plan = _plan_with_package_feature_gate(
        tmp_path,
        {
            "default": ["first"],
            "first": ["second"],
            "second": ["gate"],
            "gate": [],
        },
    )

    contract = next(
        target for target in plan.selected_targets if target.name == "compile_contract"
    )

    assert "gate" in contract.enabled_features, (
        "transitive package feature activation must reach target metadata"
    )


def test_package_feature_cycle_terminates_and_enables_target(
    tmp_path: pathlib.Path,
) -> None:
    """A feature cycle stops at visited features while preserving reachability."""
    plan = _plan_with_package_feature_gate(
        tmp_path,
        {
            "default": ["first"],
            "first": ["second"],
            "second": ["first", "gate"],
            "gate": [],
        },
    )

    contract = next(
        target for target in plan.selected_targets if target.name == "compile_contract"
    )

    assert "gate" in contract.enabled_features, (
        "the cycle guard must not prevent other activated features"
    )


def test_dependency_activation_does_not_enable_package_feature(
    tmp_path: pathlib.Path,
) -> None:
    """A dep: activation is not a package-local target feature."""
    plan = _plan_with_package_feature_gate(
        tmp_path,
        {"default": ["first"], "first": ["dep:gate"], "gate": []},
    )

    assert not any(
        target.name == "compile_contract" for target in plan.selected_targets
    ), "dep: activation must not satisfy a package target's required feature"


def test_dependency_feature_activation_does_not_enable_package_feature(
    tmp_path: pathlib.Path,
) -> None:
    """A slash-qualified dependency activation is not package-local."""
    plan = _plan_with_package_feature_gate(
        tmp_path,
        {"default": ["first"], "first": ["dependency/gate"], "gate": []},
    )

    assert not any(
        target.name == "compile_contract" for target in plan.selected_targets
    ), "dependency feature activation must not satisfy a package target gate"


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


def test_explicit_target_selection_preserves_cargo_feature_error(
    tmp_path: pathlib.Path,
) -> None:
    """An explicit gated target reaches Cargo to preserve its feature error."""
    options = parse_cargo_test_options(["--no-default-features", "--bin", "podbot"])

    plan = create_test_plan(package_document(tmp_path), options)

    assert [target.name for target in plan.selected_targets] == ["podbot"], (
        "an explicitly named disabled target must not be silently dropped"
    )
    assert plan.ordinary_target_args == ("--bin", "podbot"), (
        "Cargo must receive the explicit selector and report its normal error"
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


@pytest.mark.parametrize(
    ("argument", "common", "package_specs"),
    [
        ("-j8", ("-j", "8"), ()),
        ("-Finternal", ("-F", "internal"), ()),
        ("-F=internal", ("-F", "internal"), ()),
        ("-ppodbot", ("-p", "podbot"), ("podbot",)),
        ("-p=podbot", ("-p", "podbot"), ("podbot",)),
    ],
)
def test_attached_short_value_options_are_normalized(
    argument: str,
    common: tuple[str, ...],
    package_specs: tuple[str, ...],
) -> None:
    """Attached short-option values map to the same phases as separated ones."""
    options = parse_cargo_test_options([argument])

    assert options.common == common, "the option and value must remain explicit"
    assert options.package_specs == package_specs, (
        "attached package selection must remain package-scoped"
    )


@pytest.mark.parametrize("target_name", ["cli_feature_gating", "compile_contract"])
def test_registry_rejects_a_removed_target(
    tmp_path: pathlib.Path, target_name: str
) -> None:
    """The registry cannot silently outlive a renamed or removed test target."""
    metadata = package_document(tmp_path)
    metadata["packages"][0]["targets"] = [
        target
        for target in metadata["packages"][0]["targets"]
        if target["name"] != target_name
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
