"""The main-branch coverage publisher provisions the MSRV toolchain.

The UI verification tests resolve the toolchain named by `rust-version`
through Rustup and never install it, so a lane that runs the coverage suite
without an earlier test step must install it first. The contract reads the
parsed workflow and checks the order and that the version comes from the
manifest rather than a restated number.
"""

from __future__ import annotations

import re
from pathlib import Path

import yaml

PUBLISHER = Path(__file__).resolve().parents[2] / ".github/workflows/coverage-main.yml"


def _steps() -> list[dict[str, object]]:
    """Return the publisher's first job's steps."""
    document = yaml.safe_load(PUBLISHER.read_text(encoding="utf-8"))
    jobs = document["jobs"]
    return list(next(iter(jobs.values()))["steps"])


def test_the_publisher_installs_the_msrv_before_measuring_coverage() -> None:
    """Install the manifest's MSRV toolchain ahead of generate-coverage."""
    steps = _steps()
    install = [
        index
        for index, step in enumerate(steps)
        if "rustup toolchain install" in str(step.get("run", ""))
    ]
    measure = [
        index
        for index, step in enumerate(steps)
        if "/generate-coverage@" in str(step.get("uses", ""))
    ]
    assert install, "the publisher must install the MSRV toolchain"
    assert measure, "the publisher must measure coverage"
    assert install[0] < measure[0], "the MSRV must be installed before coverage"
    command = str(steps[install[0]]["run"])
    assert "rust_version" in command, "the MSRV must come from the manifest"
    assert "pipefail" in command, "a failed cargo metadata must stop the step"
    assert not re.search(r"\b1\.\d+\.\d+\b", command), "no restated version"
