"""Reusable Cargo metadata and artifact documents for runner tests."""

from __future__ import annotations

import copy
import dataclasses
import json
import pathlib
import textwrap
import typing as typ

import pytest


@dataclasses.dataclass(frozen=True)
class TargetProperties:
    """Describe Cargo target flags used by workspace metadata fixtures."""

    test: bool = False
    bench: bool = False
    doctest: bool = False
    features: tuple[str, ...] = ()


@dataclasses.dataclass(frozen=True)
class FakeCargoConfiguration:
    """Set exit outcomes and workspace metadata for one fake Cargo run."""

    build_exit: int = 0
    test_exit: int = 0
    ordinary_exit: int = 0
    doctest_exit: int = 0
    metadata: dict[str, typ.Any] | None = None


def package_document(root: pathlib.Path) -> dict[str, typ.Any]:
    """Return a workspace metadata document with each Cargo target kind."""
    package_id = "path+file:///workspace/podbot#podbot@0.1.0"
    targets = [
        _target(
            "podbot", ["lib"], TargetProperties(test=True, bench=True, doctest=True)
        ),
        _target(
            "podbot",
            ["bin"],
            TargetProperties(test=True, bench=True, features=("cli",)),
        ),
        _target("compile_contract", ["test"], TargetProperties(test=True)),
        _target("cli_feature_gating", ["test"], TargetProperties(test=True)),
        _target("example_check", ["example"], TargetProperties(test=True)),
        _target("benchmarks", ["bench"], TargetProperties(bench=True)),
    ]
    return {
        "workspace_root": str(root),
        "target_directory": str(root / "target"),
        "workspace_members": [package_id],
        "workspace_default_members": [package_id],
        "packages": [
            {
                "id": package_id,
                "name": "podbot",
                "version": "0.1.0",
                "manifest_path": str(root / "Cargo.toml"),
                "features": {
                    "default": ["default-cli"],
                    "default-cli": ["cli"],
                    "cli": ["dep:clap"],
                    "internal": [],
                    "experimental": [],
                },
                "authors": ["Podbot Authors"],
                "description": "A test package",
                "repository": "https://example.invalid/podbot",
                "license": "MIT",
                "license_file": None,
                "homepage": None,
                "rust_version": "1.88",
                "categories": [],
                "keywords": [],
                "targets": targets,
            }
        ],
    }


def fake_cargo_environment(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    configuration: FakeCargoConfiguration = FakeCargoConfiguration(),
) -> dict[str, str]:
    """Write fake Cargo and current-build test executables for runner tests."""
    metadata_path = tmp_path / "metadata.json"
    metadata_path.write_text(
        json.dumps(configuration.metadata or package_document(tmp_path)),
        encoding="utf-8",
    )
    cargo_path = tmp_path / "fake-cargo.py"
    _write_fake_cargo_program(cargo_path)
    values = {
        "FAKE_METADATA": str(metadata_path),
        "FAKE_CARGO": str(cargo_path),
        "FAKE_COMMANDS": str(tmp_path / "commands.jsonl"),
        "FAKE_EXECUTABLE": str(tmp_path / "target/debug/deps/compile_contract-current"),
        "FAKE_BUILD_RETURNED": str(tmp_path / "build-returned"),
        "FAKE_TEST_ARGS": str(tmp_path / "test-args.json"),
        "FAKE_BUILD_EXIT": str(configuration.build_exit),
        "FAKE_TEST_EXIT": str(configuration.test_exit),
        "FAKE_ORDINARY_EXIT": str(configuration.ordinary_exit),
        "FAKE_DOCTEST_EXIT": str(configuration.doctest_exit),
    }
    for name, value in values.items():
        monkeypatch.setenv(name, value)
    return values


def _write_fake_cargo_program(cargo_path: pathlib.Path) -> None:
    """Write the subprocess protocol used by test-runner execution tests."""
    cargo_path.write_text(
        textwrap.dedent(
            """\
            #!/usr/bin/env python3
            import json
            import os
            import pathlib
            import sys
            import time

            args = sys.argv[1:]
            if args[0] == "metadata":
                print(pathlib.Path(os.environ["FAKE_METADATA"]).read_text())
                raise SystemExit(0)
            with pathlib.Path(os.environ["FAKE_COMMANDS"]).open("a") as commands:
                commands.write(json.dumps(args) + "\\n")
            if "--no-run" in args:
                status = int(os.environ["FAKE_BUILD_EXIT"])
                if status:
                    raise SystemExit(status)
                test_targets = [
                    args[index + 1]
                    for index, argument in enumerate(args[:-1])
                    if argument == "--test"
                ]
                for target_name in test_targets:
                    executable = pathlib.Path(os.environ["FAKE_EXECUTABLE"])
                    executable = executable.with_name(f"{target_name}-current")
                    executable.parent.mkdir(parents=True, exist_ok=True)
                    executable.write_text(
                        "#!/usr/bin/env python3\\n"
                        "import json, os, pathlib, sys\\n"
                        "if os.environ.get('FAKE_REQUIRE_BUILD_RETURNED') == 'true' and not pathlib.Path(os.environ['FAKE_BUILD_RETURNED']).exists():\\n"
                        "    raise SystemExit(41)\\n"
                        "pathlib.Path(os.environ['FAKE_TEST_ARGS']).write_text(json.dumps(sys.argv[1:]))\\n"
                        "raise SystemExit(int(os.environ['FAKE_TEST_EXIT']))\\n"
                    )
                    executable.chmod(0o755)
                    message = {
                        "reason": "compiler-artifact",
                        "package_id": "path+file:///workspace/podbot#podbot@0.1.0",
                        "target": {"name": target_name, "kind": ["test"]},
                        "profile": {"test": True, "debug_assertions": True},
                        "executable": str(executable),
                        "filenames": [str(executable)],
                    }
                    print(json.dumps(message), flush=True)
                time.sleep(0.05)
                raise SystemExit(0)
            if "--doc" in args:
                raise SystemExit(int(os.environ["FAKE_DOCTEST_EXIT"]))
            raise SystemExit(int(os.environ["FAKE_ORDINARY_EXIT"]))
            """
        ),
        encoding="utf-8",
    )
    cargo_path.chmod(0o755)


def workspace_with_sibling_package(root: pathlib.Path) -> dict[str, typ.Any]:
    """Add a sibling package with a same-named nested Cargo target."""
    metadata = package_document(root)
    sibling = copy.deepcopy(metadata["packages"][0])
    sibling["id"] = "path+file:///workspace/sibling#sibling@0.1.0"
    sibling["name"] = "sibling"
    sibling["manifest_path"] = str(root / "sibling" / "Cargo.toml")
    metadata["workspace_members"].append(sibling["id"])
    metadata["workspace_default_members"].append(sibling["id"])
    metadata["packages"].append(sibling)
    return metadata


def _target(
    name: str,
    kinds: list[str],
    properties: TargetProperties = TargetProperties(),
) -> dict[str, typ.Any]:
    """Build a Cargo metadata target row."""
    return {
        "name": name,
        "kind": kinds,
        "test": properties.test,
        "bench": properties.bench,
        "doctest": properties.doctest,
        "required-features": list(properties.features),
    }
