"""The run-block reader accepts only a lone command that must run.

The contract step in `ci.yml` is required through this reader. A substring
test passed for `echo make test-workflow-contracts` and for a comment, and a
reader of simple commands still passed for
`false && make test-workflow-contracts`: in each the lane could stop running
the contracts while they stayed green.

Run via ``make test-workflow-contracts``.
"""

import pytest
from shell_commands import runs_unconditionally

COMMAND = "make test-workflow-contracts"


@pytest.mark.parametrize(
    "script",
    [
        pytest.param("make test-workflow-contracts", id="alone"),
        pytest.param("make  test-workflow-contracts", id="extra-whitespace"),
        pytest.param("make test-workflow-contracts\n", id="a-trailing-newline"),
        pytest.param(
            "RUSTFLAGS=-Dwarnings make test-workflow-contracts",
            id="after-an-assignment",
        ),
        pytest.param("make \\\n  test-workflow-contracts", id="across-a-continuation"),
        pytest.param("make test-workflow-contracts # measure", id="before-a-comment"),
        pytest.param(
            "# measure\nmake test-workflow-contracts", id="after-a-comment-line"
        ),
        pytest.param(
            "make test-workflow-contracts COVERAGE_OUTPUT=x", id="with-arguments"
        ),
    ],
)
def test_a_lone_command_is_accepted(script: str) -> None:
    """Each shape runs the command, and only it, whenever the step runs."""
    assert runs_unconditionally(script, COMMAND), f"{script!r} runs {COMMAND!r}"


@pytest.mark.parametrize(
    "script",
    [
        pytest.param("false && make test-workflow-contracts", id="after-a-failing-and"),
        pytest.param(
            "true || make test-workflow-contracts", id="after-a-succeeding-or"
        ),
        pytest.param("exit 0; make test-workflow-contracts", id="after-an-exit"),
        pytest.param("make test-workflow-contracts &", id="in-the-background"),
        pytest.param("make test-workflow-contracts | tee log", id="in-a-pipeline"),
        pytest.param("(make test-workflow-contracts)", id="in-a-subshell"),
        pytest.param(
            "make test-workflow-contracts\necho done", id="beside-another-command"
        ),
        pytest.param("echo make test-workflow-contracts", id="an-argument"),
        pytest.param("# make test-workflow-contracts", id="a-comment"),
        pytest.param(
            "make test # ; make test-workflow-contracts",
            id="an-operator-inside-a-comment",
        ),
        pytest.param("make test-workflow-contracts-report", id="a-longer-target"),
        pytest.param(
            "printf '%s' 'make test-workflow-contracts'", id="a-quoted-string"
        ),
        pytest.param("make 'test-workflow-contracts", id="an-unbalanced-quote"),
        pytest.param(
            "make test-workflow-contracts\necho 'oops", id="beside-an-unreadable-line"
        ),
        pytest.param("", id="nothing"),
    ],
)
def test_anything_else_is_refused(script: str) -> None:
    """The narrow half, and the reason the reader exists.

    Every row contains the words. The first five do not run the command to
    completion, or not at all; the next two might, but the reader refuses
    rather than reason about what else a step does, which is why the lane gives
    the command a step of its own; the rest only mention it. A line the reader
    cannot tokenize refuses the whole script rather than being skipped, which
    is what the last row but one proves.
    """
    assert not runs_unconditionally(script, COMMAND), f"{script!r} is refused"
