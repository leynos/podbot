"""Test the sccache health checker the gha lanes run after recording statistics.

Run via ``make workflow-contracts``.
"""

from __future__ import annotations

import importlib.util
import json
import types
import typing as typ
from pathlib import Path

import pytest

SCRIPTS = Path(__file__).resolve().parents[1]


@pytest.fixture(scope="module")
def checker() -> types.ModuleType:
    """Import the health checker script as a module."""
    spec = importlib.util.spec_from_file_location(
        "check_sccache_health", SCRIPTS / "check_sccache_health.py"
    )
    if spec is None or spec.loader is None:
        message = "could not load the sccache health checker"
        raise ImportError(message)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _document(
    location: str = "ghac, name: sccache-v0.17.0", **stats: object
) -> dict[str, object]:
    """Return a statistics document shaped like `sccache --stats-format json`.

    The defaults are the healthy shape of run 35815203116's `coverage-check`.
    """
    healthy: dict[str, object] = {
        "compile_requests": 3256,
        "cache_hits": {"counts": {"Rust": 1505, "C/C++": 912}},
        "cache_misses": {"counts": {"Rust": 69}},
        "cache_writes": 69,
        "cache_write_errors": 0,
        "cache_read_errors": 0,
        "cache_timeouts": 0,
        "cache_errors": {"counts": {}},
    }
    return {"cache_location": location, "stats": healthy | stats}


def test_a_healthy_lane_passes_quietly(checker: typ.Any) -> None:
    """The measured warm run: no failure and no warning."""
    assessment = checker.assess(_document(), "ghac")
    assert assessment.failures == ()
    assert assessment.warnings == ()


@pytest.mark.parametrize(
    ("document", "expected"),
    [
        pytest.param(
            _document(location='Local disk: "/home/runner/.cache/sccache"'),
            "not the 'ghac' backend",
            id="bound-local-disk",
        ),
        pytest.param(
            _document(compile_requests=0), "wrapped nothing", id="wrapped-nothing"
        ),
        pytest.param(
            _document(cache_writes=0, cache_write_errors=3788),
            "every store failed",
            id="every-store-failed",
        ),
        pytest.param(
            _document(cache_hits={"counts": {}}, cache_read_errors=12),
            "every read failed",
            id="every-read-failed",
        ),
    ],
)
def test_a_broken_integration_fails(
    checker: typ.Any, document: dict[str, object], expected: str
) -> None:
    """Each structural signature fails the lane on its own."""
    failures = checker.assess(document, "ghac").failures
    assert any(expected in failure for failure in failures), failures


@pytest.mark.parametrize(
    ("stats", "expected"),
    [
        pytest.param(
            {"cache_write_errors": 1}, "cache_write_errors is 1", id="one-write-error"
        ),
        pytest.param(
            {"cache_read_errors": 2}, "cache_read_errors is 2", id="read-errors"
        ),
        pytest.param({"cache_timeouts": 1}, "cache_timeouts is 1", id="a-timeout"),
        pytest.param(
            {"cache_errors": {"counts": {"Rust": 2}}},
            "cache_errors is 2",
            id="cache-errors",
        ),
    ],
)
def test_isolated_errors_warn_without_failing(
    checker: typ.Any, stats: dict[str, object], expected: str
) -> None:
    """A proxy hiccup costs a compile, not a red pull request.

    The first cold run on this backend had one write error against 1,572
    misses; failing on that would make the lane as flaky as the service it
    talks to.
    """
    assessment = checker.assess(_document(**stats), "ghac")
    assert assessment.failures == ()
    assert expected in assessment.warnings


def test_a_local_lane_is_judged_against_its_own_backend(checker: typ.Any) -> None:
    """The expected prefix is an argument, so the local lanes can use it too."""
    local = _document(location='Local disk: "/home/runner/.cache/sccache"')
    assert checker.assess(local, "Local disk").failures == ()


@pytest.mark.parametrize(
    "case",
    [
        pytest.param((json.dumps(_document()), 0), id="healthy"),
        pytest.param((json.dumps(_document(compile_requests=0)), 1), id="broken"),
        pytest.param(("not json", 1), id="unreadable"),
        pytest.param(("[]", 1), id="not-an-object"),
    ],
)
def test_the_exit_status_follows_the_assessment(
    checker: typ.Any,
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    case: tuple[str, int],
) -> None:
    """The job fails exactly when the assessment does, and says why."""
    contents, status = case
    statistics = tmp_path / "sccache-stats.json"
    statistics.write_text(contents, encoding="utf-8")
    assert checker.main(["--expect-location", "ghac", str(statistics)]) == status
    output = capsys.readouterr().out
    assert ("::error::" in output) is (status == 1)


def test_a_missing_file_fails(checker: typ.Any, tmp_path: Path) -> None:
    """No statistics is a broken lane, not a pass."""
    assert (
        checker.main(["--expect-location", "ghac", str(tmp_path / "absent.json")]) == 1
    )
