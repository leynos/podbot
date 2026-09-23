"""The closure's properties over generated call chains, through the rule's query.

The table cases in ``pull_request_closure_test`` fix one and two links.
Here Hypothesis draws chains of any length, each link spelt with either
local prefix, the probe's road to CodeScene drawn from several, and
decoy workflows nobody calls. The query is ``pull_request_workflows``
itself, the one every CV-005 clause runs over, so a property proved
here is a property of the rule rather than of a helper beside it.

Documents are generated as workflow-shaped text and parsed by
``load_workflow``, so every generated value lands where the readers
look.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import typing as typ

from codescene_coverage import pull_request_workflows
from codescene_reach import codescene_contacts, token_sites
from hypothesis import given
from hypothesis import strategies as st
from workflow_reading import load_workflow

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    from workflow_reading import WorkflowDocument

#: The probe's last step, by road; each reaches CodeScene with the secret.
ROADS: typ.Final[dict[str, str]] = {
    "run": "      - run: curl -H ${{ secrets.CS_ACCESS_TOKEN }} https://codescene.io\n",
    "input": (
        "      - uses: x/y@v1\n        with:\n"
        "          url: https://codescene.io\n"
        "          token: ${{ secrets.CS_ACCESS_TOKEN }}\n"
    ),
    "env": (
        "      - env:\n          U: https://CODESCENE.io\n"
        "          T: ${{ secrets.CS_ACCESS_TOKEN }}\n        run: curl $U\n"
    ),
}


def _link(prefix: str, target: str) -> str:
    """Return a job calling one local workflow by the given prefix."""
    return f"jobs:\n  next:\n    uses: {prefix}.github/workflows/{target}\n"


@st.composite
def _chain(draw: st.DrawFn) -> tuple[dict[str, WorkflowDocument], list[str]]:
    """Draw a pull-request caller, a chain of callees ending in a probe, and decoys."""
    length = draw(st.integers(min_value=1, max_value=6))
    names = [f"link{index}.yml" for index in range(length)]
    prefixes = draw(
        st.lists(st.sampled_from(["./", "$/"]), min_size=length, max_size=length)
    )
    road = draw(st.sampled_from(sorted(ROADS)))
    documents = {
        "ci.yml": load_workflow(f"on: [pull_request]\n{_link(prefixes[0], names[0])}")
    }
    for index, name in enumerate(names):
        body = (
            _link(prefixes[index + 1], names[index + 1])
            if index + 1 < length
            else f"jobs:\n  probe:\n    steps:\n{ROADS[road]}"
        )
        documents[name] = load_workflow(f"on:\n  workflow_call:\n{body}")
    decoys = draw(st.integers(min_value=0, max_value=3))
    for index in range(decoys):
        documents[f"decoy{index}.yml"] = load_workflow(
            f"on:\n  workflow_call:\njobs:\n  d:\n    steps:\n{ROADS[road]}"
        )
    return documents, ["ci.yml", *names]


@given(_chain())
def test_the_lane_is_exactly_the_chain(
    drawn: tuple[dict[str, WorkflowDocument], list[str]],
) -> None:
    """Every link is in the lane, however long the chain, and no decoy is."""
    documents, chain = drawn
    assert sorted(pull_request_workflows(documents)) == sorted(chain)


@given(_chain())
def test_the_probe_is_caught_at_the_end_of_any_chain(
    drawn: tuple[dict[str, WorkflowDocument], list[str]],
) -> None:
    """The token and host clauses both reach the last link, and only it."""
    documents, chain = drawn
    lane = pull_request_workflows(documents)
    tokens = {site.split(":")[0] for n, d in lane.items() for site in token_sites(n, d)}
    hosts = {
        site.split(":")[0] for n, d in lane.items() for site in codescene_contacts(n, d)
    }
    assert tokens == {chain[-1]}
    assert hosts == {chain[-1]}
