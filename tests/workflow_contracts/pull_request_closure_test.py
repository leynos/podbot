"""The pull-request lane as a closure, and every road to CodeScene from it.

A workflow declaring only ``workflow_call`` still runs on a pull request
when a pull-request workflow calls it, and ``secrets: inherit`` hands it
the token. Each case drives the production query,
``pull_request_workflows``, over constructed documents rather than
proving the closure on a helper the rule does not call: this
repository's own workflows hold no local call, so they could not tell a
working closure from a trigger list.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import typing as typ

import pytest
from codescene_coverage import is_refused_call, pull_request_workflows
from codescene_reach import codescene_contacts, token_sites
from workflow_reading import load_workflow, serves_pull_requests

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    from collections.abc import Callable

    from workflow_reading import WorkflowDocument

#: A ``workflow_call``-only workflow reaching CodeScene with an
#: inherited secret. It names no action and runs no ``cs-coverage``.
PROBE: typ.Final[str] = (
    "on:\n  workflow_call:\n"
    "jobs:\n  probe:\n    steps:\n"
    "      - run: |\n"
    '          curl -H "Authorization: ${{ secrets.CS_ACCESS_TOKEN }}" \\\n'
    "            https://api.codescene.io/v2/projects/75835\n"
)


def _caller(reference: str) -> str:
    """Return a pull-request workflow calling one reference with inherited secrets."""
    return (
        "on:\n  pull_request:\n"
        f"jobs:\n  call:\n    uses: {reference}\n    secrets: inherit\n"
    )


def _lane_sites(
    documents: dict[str, WorkflowDocument],
    reading: Callable[[str, WorkflowDocument], list[str]],
) -> list[str]:
    """Return one reading's sites over the pull-request closure, as the rule runs it."""
    return sorted(
        site
        for name, document in pull_request_workflows(documents).items()
        for site in reading(name, document)
    )


@pytest.mark.parametrize(
    "prefix",
    [pytest.param("./", id="dot-slash"), pytest.param("$/", id="dollar-slash")],
)
def test_the_probe_is_caught_through_either_local_spelling(prefix: str) -> None:
    """Both documented local spellings put the callee in the lane.

    The trigger-only reading reaches ``ci.yml`` alone; the closure puts
    the probe in reach of the token clause, along with the caller's
    ``secrets: inherit``, and of the host clause. The two clauses travel
    together: run over a trigger list they would share one blind spot.
    """
    documents = {
        "ci.yml": load_workflow(_caller(f"{prefix}.github/workflows/probe.yml")),
        "probe.yml": load_workflow(PROBE),
    }
    assert [n for n, d in documents.items() if serves_pull_requests(d)] == ["ci.yml"]
    assert _lane_sites(documents, token_sites) == [
        "ci.yml: jobs.call.secrets",
        "probe.yml: jobs.probe.steps[0].run",
    ]
    assert _lane_sites(documents, codescene_contacts) == [
        "probe.yml: jobs.probe.steps[0].run"
    ]


def test_the_closure_is_transitive() -> None:
    """A probe two calls away is in the lane as surely as one call away."""
    documents = {
        "ci.yml": load_workflow(_caller("./.github/workflows/middle.yml")),
        "middle.yml": load_workflow(
            "on:\n  workflow_call:\n"
            "jobs:\n  next:\n    uses: $/.github/workflows/probe.yml\n"
        ),
        "probe.yml": load_workflow(PROBE),
    }
    assert sorted(pull_request_workflows(documents)) == [
        "ci.yml",
        "middle.yml",
        "probe.yml",
    ]


@pytest.mark.parametrize(
    "reference",
    [
        pytest.param("./scripts/probe.yml", id="outside-the-workflow-directory"),
        pytest.param("./.github/workflows/nested/probe.yml", id="nested"),
        pytest.param("./.github/workflows/absent.yml", id="no-such-workflow"),
        pytest.param("other/repo/.github/workflows/probe.yml@main", id="other-repo"),
        pytest.param("$/.github/workflows/probe.yml@main", id="local-with-a-ref"),
    ],
)
def test_a_reference_of_the_wrong_shape_is_not_followed(reference: str) -> None:
    """The shape match is narrow as well as broad.

    GitHub calls nothing outside the workflow directory, another
    repository's file is not in this tree, and a local path with a ref
    names a version this tree does not hold.
    """
    documents = {
        "ci.yml": load_workflow(_caller(reference)),
        "probe.yml": load_workflow(PROBE),
    }
    assert sorted(pull_request_workflows(documents)) == ["ci.yml"], reference


@pytest.mark.parametrize(
    ("reference", "refused"),
    [
        pytest.param("leynos/podbot/.github/workflows/x.yml@main", True, id="self"),
        pytest.param("Leynos/Podbot/.github/workflows/x.yml@v1", True, id="self-case"),
        pytest.param("$/.github/workflows/x.yml@main", True, id="dollar-ref"),
        pytest.param("./.github/workflows/x.yml@main", True, id="dot-ref"),
        pytest.param("$/.github/workflows/x.yml", False, id="dollar"),
        pytest.param("./.github/workflows/x.yml", False, id="dot"),
        pytest.param(
            "leynos/shared-actions/.github/workflows/x.yml@a", False, id="other"
        ),
    ],
)
def test_a_self_call_at_a_ref_is_refused(reference: str, *, refused: bool) -> None:
    """A self-call at a ref runs a version the closure cannot read."""
    assert is_refused_call(reference) is refused, reference


def test_a_workflow_nothing_calls_is_not_in_the_lane() -> None:
    """The closure is narrow: an uncalled ``workflow_call`` workflow is outside it."""
    documents = {
        "ci.yml": load_workflow("on:\n  pull_request:\njobs:\n  a:\n    steps: []\n"),
        "probe.yml": load_workflow(PROBE),
    }
    assert sorted(pull_request_workflows(documents)) == ["ci.yml"]


#: Every road the secret can take into a process, and where it is found.
TOKEN_ROADS: typ.Final[list[tuple[str, str, str]]] = [
    (
        "run-body",
        "jobs:\n  a:\n    steps:\n      - run: echo ${{ secrets.CS_ACCESS_TOKEN }}\n",
        "jobs.a.steps[0].run",
    ),
    (
        "index-syntax",
        "jobs:\n  a:\n    steps:\n      - run: echo ${{ secrets['cs_access_token'] }}\n",
        "jobs.a.steps[0].run",
    ),
    (
        "action-input",
        "jobs:\n  a:\n    steps:\n      - uses: x/y@v1\n"
        "        with:\n          token: ${{ secrets.CS_ACCESS_TOKEN }}\n",
        "jobs.a.steps[0].with.token",
    ),
    (
        "env-value-renamed",
        "env:\n  T: ${{ secrets.CS_ACCESS_TOKEN }}\njobs: {}\n",
        "env.T",
    ),
    (
        "env-key",
        "jobs:\n  a:\n    env:\n      CS_ACCESS_TOKEN: x\n",
        "jobs.a.env.CS_ACCESS_TOKEN<key>",
    ),
    (
        "named-forwarding",
        "jobs:\n  a:\n    uses: o/r/.github/workflows/x.yml@v1\n"
        "    secrets:\n      CS_ACCESS_TOKEN: ${{ secrets.CS_ACCESS_TOKEN }}\n",
        "jobs.a.secrets.CS_ACCESS_TOKEN<key>",
    ),
    (
        "inherit",
        "jobs:\n  a:\n    uses: o/r/.github/workflows/x.yml@v1\n    secrets: inherit\n",
        "jobs.a.secrets",
    ),
    (
        "callee-declaration",
        "on:\n  workflow_call:\n    secrets:\n      CS_ACCESS_TOKEN:\n        required: true\n",
        "on.workflow_call.secrets.CS_ACCESS_TOKEN<key>",
    ),
]


@pytest.mark.parametrize(
    ("body", "site"),
    [pytest.param(body, site, id=road) for road, body, site in TOKEN_ROADS],
)
def test_the_token_clause_reads_every_road(body: str, site: str) -> None:
    """Each road puts the secret in reach, so each is found where it is."""
    assert f"ci.yml: {site}" in token_sites("ci.yml", load_workflow(body)), body


@pytest.mark.parametrize(
    "body",
    [
        pytest.param("name: CS_ACCESS_TOKEN is main's alone\n", id="prose"),
        pytest.param(
            "jobs:\n  a:\n    env:\n      T: ${{ secrets.OTHER }}\n", id="other"
        ),
        pytest.param("# ${{ secrets.CS_ACCESS_TOKEN }}\nname: x\n", id="a-comment"),
    ],
)
def test_the_token_clause_ignores_a_mention(body: str) -> None:
    """The clause is narrow: prose naming the secret is not a read of it."""
    assert token_sites("ci.yml", load_workflow(body)) == []


@pytest.mark.parametrize(
    ("body", "where"),
    [
        pytest.param("env:\n  U: https://codescene.io\n", "env.U", id="workflow-env"),
        pytest.param(
            "defaults:\n  run:\n    shell: curl https://codescene.io; bash {0}\n",
            "defaults.run.shell",
            id="default-shell",
        ),
        pytest.param(
            "jobs:\n  a:\n    uses: ./.github/workflows/x.yml\n"
            "    with:\n      url: https://API.CodeScene.IO\n",
            "jobs.a.with.url",
            id="call-input-upper-case",
        ),
        pytest.param(
            "jobs:\n  a:\n    services:\n      s:\n        env:\n"
            "          U: https://codescene.io\n",
            "jobs.a.services.s.env.U",
            id="service-env",
        ),
    ],
)
def test_the_host_clause_reads_the_whole_document(body: str, where: str) -> None:
    """Every scalar is read, case-folded, so no scope is a way round the rule."""
    assert codescene_contacts("ci.yml", load_workflow(body)) == [f"ci.yml: {where}"]
