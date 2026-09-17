"""What the workflow files must say, and why each rule exists.

Three rules, each written from a specific failure rather than from a
general wish for tidiness.

The **hermetic pin** rule exists because every `leynos/shared-actions`
reference here sat, for two months, on a commit whose `setup-rust`
installs sccache, starts it, and exports no `RUSTC_WRAPPER`. Cargo
routed no compilation through it, so the cache was downloaded on every
Rust job and cached nothing, and the only visible symptom was a slow
lane. Dependabot's open group bump (#164) proposes moving those
references three weeks forward to a commit that still predates the
export, which reads exactly like pins being brought up to date. That is
why the class is refused by name rather than by review: recency is not
the property that matters.

The **watchdog** rule exists because the budget was inherited. An
inherited default is a value this repository never states and cannot
notice changing.

The **placement** rule exists because a folded scalar whose continuation
is indented more deeply than its first line keeps the line break, and the
resulting `runs-on` carries a newline inside the expression. GitHub
evaluates it anyway, so a green run is not evidence the defect is absent
and nothing but a contract will find it.

The last of those cannot be proved by this repository's own files: every
job here names a literal runner, so a check parametrized over them passes
whether or not it discriminates anything. The mechanism is driven
directly with constructed documents, in both directions, and the real
files are asserted separately.
"""

from __future__ import annotations

import re
import sys
import typing as typ
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from workflow_contracts import (  # noqa: E402
    COVERAGE_ACTION,
    WATCHDOG_VARIABLE,
    WRAPPER_LESS_PINS,
    load_workflow_documents,
    of_type,
    shared_actions_references,
)
from workflow_contracts import parse as parse_workflow  # noqa: E402
from workflow_contracts import coverage_jobs as coverage_jobs_in  # noqa: E402
from workflow_placement import (  # noqa: E402
    line_break_fault,
    runs_on_declarations,
)

#: A 40-hex commit, which is the only form a reference may take. A tag or
#: a branch is mutable, and a short SHA is ambiguous.
COMMIT_SHA = re.compile(r"\A[0-9a-f]{40}\Z")

#: The command CI must run to execute the contracts in this directory.
CONTRACT_COMMAND: typ.Final[str] = "make workflow-contracts"

#: The budget this repository runs its coverage steps under, in seconds.
#: It is the action's current default, declared here so that a later
#: change to that default cannot move it silently. The worst coverage
#: step observed over six green runs is 487 s, so this is about 3.7
#: times the worst seen, and both coverage jobs pass
#: `use-cargo-nextest: 'false'`, which leaves the watchdog as the only
#: timer bounding cargo.
REQUIRED_WATCHDOG: typ.Final[str] = "1800"


@pytest.fixture(name="workflow_texts")
def fixture_workflow_texts() -> dict[str, str]:
    """Return every workflow file's text.

    Returns
    -------
    dict[str, str]
        File name to file text.
    """
    return load_workflow_documents()


def test_every_shared_actions_reference_is_a_commit(
    workflow_texts: dict[str, str],
) -> None:
    """A mutable ref is not a pin, and a short one is not unique."""
    references = shared_actions_references(workflow_texts)

    assert references, (
        "no shared-actions reference was found at all; the reader matches by "
        "repository prefix, so an empty result means the reader is broken "
        "rather than that the workflows stopped using the repository"
    )
    for reference in references:
        assert COMMIT_SHA.match(reference.ref), (
            f"{reference.workflow} pins {reference.path} at {reference.ref!r}, "
            f"which is not a 40-hex commit"
        )


def test_every_shared_actions_reference_moves_together(
    workflow_texts: dict[str, str],
) -> None:
    """One SHA across the repository, so a partial repin cannot land.

    The references were on two different commits before this contract
    existed, two months apart, and nothing said so. Actions from one
    repository are developed together and the interfaces between them
    move together; a pull request repinning some of them leaves a
    combination nobody has run.
    """
    references = shared_actions_references(workflow_texts)
    refs = {reference.ref for reference in references}

    assert len(refs) == 1, (
        "every leynos/shared-actions reference must name one commit; these "
        "do not: "
        + ", ".join(
            f"{reference.workflow}:{reference.path.rsplit('/', 1)[-1]}="
            f"{reference.ref[:8]}"
            for reference in references
        )
    )


def test_no_reference_names_a_wrapper_less_pin(
    workflow_texts: dict[str, str],
) -> None:
    """The known pins that install sccache and use it for nothing.

    Named individually rather than bounded by date, because the property
    that matters is not recency: it is whether that tree's `setup-rust`
    exports `RUSTC_WRAPPER`. A newer commit lacking the export would be
    just as wrong, and would be caught by the measurement rather than
    here, but these six are the ones this repository can reach today:
    the two it sat on until the repin, and four a dependency bump or a
    revert can propose.
    """
    for reference in shared_actions_references(workflow_texts):
        assert reference.ref not in WRAPPER_LESS_PINS, (
            f"{reference.workflow} pins {reference.path} at "
            f"{reference.ref[:8]}, whose setup-rust exports no RUSTC_WRAPPER; "
            f"sccache would be installed and started on every Rust job and "
            f"used by nothing. The export landed on shared-actions at "
            f"c6125f1 on 2026-09-04; Dependabot's #164 proposes 57a33fa6, "
            f"which is newer than the old pins and still short of it"
        )


def test_every_coverage_job_states_its_watchdog(
    workflow_texts: dict[str, str],
) -> None:
    """An inherited watchdog is a budget this repository never stated.

    The value is pinned as well as required, because the action's
    default is what it replaces: leaving the assertion at "some value is
    set" would let the budget drift from the one measured here without
    failing anything.
    """
    jobs = coverage_jobs_in(workflow_texts)

    assert jobs, (
        f"no job invoking {COVERAGE_ACTION} was found; the reader matches the "
        f"action path, so an empty result means the reader is broken"
    )
    for workflow, job, watchdog in jobs:
        assert watchdog is not None, (
            f"{workflow}:{job} runs the coverage action without declaring "
            f"{WATCHDOG_VARIABLE}, so it inherits the action's default"
        )
        assert str(watchdog) == REQUIRED_WATCHDOG, (
            f"{workflow}:{job} sets {WATCHDOG_VARIABLE} to {watchdog!r}, not "
            f"the {REQUIRED_WATCHDOG}s this repository measured and states"
        )


def test_the_contracts_are_run_by_ci(workflow_texts: dict[str, str]) -> None:
    """A contract nothing runs is a comment.

    The assertion is on the command rather than on a step named
    "Workflow contracts": a step can be renamed, and a step whose `run:`
    was changed to something else would keep the name and stop asserting
    anything. It is also unguarded, so there is no `if:` that could
    leave it as dead code.
    """
    document = parse_workflow("ci.yml", workflow_texts["ci.yml"])
    steps = [
        of_type(step, dict)
        for job in of_type(document.get("jobs"), dict).values()
        for step in of_type(of_type(job, dict).get("steps"), list)
    ]
    running = [step for step in steps if CONTRACT_COMMAND in str(step.get("run", ""))]

    assert len(running) == 1, (
        f"exactly one step in ci.yml must run {CONTRACT_COMMAND!r}; {len(running)} do"
    )
    assert "if" not in running[0], (
        f"the step running {CONTRACT_COMMAND!r} is guarded by "
        f"{running[0]['if']!r}, so it can be skipped without failing anything"
    )


def test_no_runner_placement_carries_a_line_break(
    workflow_texts: dict[str, str],
) -> None:
    """The real files, asserted against the rule driven below."""
    declarations = runs_on_declarations(workflow_texts)

    assert declarations, "no job declares runs-on; the reader is broken"
    for declaration in declarations:
        fault = line_break_fault(declaration.value)
        assert fault is None, (
            f"{declaration.workflow}:{declaration.job} has a runs-on carrying "
            f"a line break, which GitHub evaluates as written. The value "
            f"parsed as {fault!r}, from:\n{declaration.raw}"
        )
