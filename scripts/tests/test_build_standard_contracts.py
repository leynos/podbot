"""The build standard's Make recipes, Cargo configuration and CI inputs.

The Concordat `rust-build-defaults` rule needs the mold linker as a Linux-only
default in `.cargo/config.toml`, every gate recipe that assigns `RUSTFLAGS` to
restate it, and the hosted gate to install it. Each lives in a file an unrelated
edit could quietly break, so each is held here.
"""

from __future__ import annotations

import os
import tomllib
import typing as typ
from pathlib import Path

import pytest
from make_dry_run import dry_run_recipe
from workflow_contracts import of_type, parse as parse_workflow

MOLD_FLAG: typ.Final[str] = "-Clink-arg=-fuse-ld=mold"
LINUX_TABLE: typ.Final[str] = 'cfg(target_os = "linux")'


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
    command = dry_run_recipe(
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
