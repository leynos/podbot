"""The repository ignores the coverage file the spelling gate writes.

`make spelling` runs pytest under `--cov`, and pytest-cov writes `.coverage`
into the repository root. Unignored, it leaves every tree dirty after a local
gate run, and a gate that reads the index cannot see it at all.

The rule is exercised at the Git boundary in a scratch repository built from
this repository's own `.gitignore`, with a control file beside it, so the
case cannot pass by Git ignoring every untracked file.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import pathlib
import shutil
import subprocess
import typing as typ

import pytest

REPOSITORY_ROOT: typ.Final[pathlib.Path] = pathlib.Path(__file__).resolve().parents[2]


def _untracked(repository: pathlib.Path) -> set[str]:
    """Return the untracked paths Git reports, ignoring any global excludes file.

    A developer's global excludes could ignore `.coverage` on their own
    machine and let this case pass without the repository's rule.
    """
    git = shutil.which("git")
    assert git, "git must be on PATH for this contract"
    result = subprocess.run(  # noqa: S603 - fixed argv, resolved executable
        [
            git,
            "-c",
            "core.excludesFile=",
            "status",
            "--porcelain",
            "--untracked-files=all",
        ],
        cwd=repository,
        capture_output=True,
        text=True,
        check=True,
    )
    return {line[3:] for line in result.stdout.splitlines() if line.startswith("?? ")}


@pytest.fixture
def scratch_repository(tmp_path: pathlib.Path) -> pathlib.Path:
    """Return an empty Git repository carrying this repository's `.gitignore`."""
    git = shutil.which("git")
    assert git, "git must be on PATH for this contract"
    subprocess.run([git, "init", "--quiet", str(tmp_path)], check=True)  # noqa: S603
    shutil.copyfile(REPOSITORY_ROOT / ".gitignore", tmp_path / ".gitignore")
    return tmp_path


def test_the_coverage_file_is_ignored_and_a_control_file_is_not(
    scratch_repository: pathlib.Path,
) -> None:
    """`.coverage` in the root is ignored; an ordinary new file is reported."""
    (scratch_repository / ".coverage").write_bytes(b"coverage data")
    (scratch_repository / "control.txt").write_text("reported\n", encoding="utf-8")

    untracked = _untracked(scratch_repository)

    assert "control.txt" in untracked, (
        f"the control file must be reported, or this case proves nothing: {untracked}"
    )
    assert ".coverage" not in untracked, (
        f"`.coverage` must be ignored by the repository's .gitignore: {untracked}"
    )
