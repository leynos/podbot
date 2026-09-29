"""Hold the Markdown formatting wiring to the estate baseline.

`make check-fmt` must run `mdtablefix --check` and `make fmt` must run
`mdtablefix --in-place`, each with the estate's Git selection and rewrite flags
and each with its exit status reaching Make; the CI job that runs
`make check-fmt` must install mdtablefix in an earlier, unconditional step; and
every markdownlint-cli2-action step must lint `**/*.md`. The rules read the
commands, not target or step names, so renaming a step or a variable cannot
satisfy them by accident.
"""

from __future__ import annotations

import itertools
import re
import shlex
import typing as typ

from workflow_reading import WorkflowDocument as Document
from workflow_reading import workflow_jobs

INSTALL_ACTION: typ.Final[str] = (
    "leynos/shared-actions/.github/actions/install-mdtablefix"
)
LINT_ACTION: typ.Final[str] = "DavidAnson/markdownlint-cli2-action"
# The selection flags choose the files; the rewrite flags choose the rewrites.
# A check without the rewrite flags passes files `make fmt` would still change.
ESTATE_FLAGS: typ.Final[frozenset[str]] = frozenset(
    {
        "--git",
        "--include-untracked",
        "--wrap",
        "--renumber",
        "--breaks",
        "--ellipsis",
        "--fences",
    }
)
MODES: typ.Final[dict[str, str]] = {"check-fmt": "--check", "fmt": "--in-place"}

_ASSIGNMENT = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\s*(?::=|\?=|=)\s*(.*)$")
_REFERENCE = re.compile(r"\$\(([A-Za-z_][A-Za-z0-9_]*)\)")
# `$(shell command -v NAME ...)` is how this estate locates a tool with a
# fallback path; the tool a recipe runs is NAME.
_TOOL_PROBE = re.compile(r"^\$\(shell\s+command\s+-v\s+([A-Za-z0-9_.-]+)")
_CONDITIONAL = re.compile(r"^(ifeq|ifneq|ifdef|ifndef)\b")
_CHECK_FMT_RUN = re.compile(r"(^|\s)make\s+(\S+\s+)*check-fmt(\s|$)")

Step = dict[str, object]


def calls(step: Step, action: str) -> bool:
    """Return whether a step's `uses:` names ``action``, at any ref."""
    uses = str(step.get("uses", ""))
    return uses.partition("@")[0].casefold() == action.casefold()


def _assignments(makefile: str) -> list[tuple[str, str]]:
    """Return each unconditional `NAME = value` pair in file order."""
    pairs: list[tuple[str, str]] = []
    depth = 0
    for line in makefile.splitlines():
        depth += bool(_CONDITIONAL.match(line)) - line.startswith("endif")
        match = _ASSIGNMENT.match(line) if depth == 0 else None
        if match:
            pairs.append((match.group(1), match.group(2).strip()))
    return pairs


def _tool_name(value: str) -> str:
    """Return the tool a `$(shell command -v NAME ...)` value locates, else it."""
    probe = _TOOL_PROBE.match(value)
    return probe.group(1) if probe else value


def variables(makefile: str) -> dict[str, str]:
    """Return each variable with exactly one unconditional assignment.

    A variable assigned twice, or inside a conditional, is left out, so a
    reference to it stays unexpanded and fails the rule rather than being
    guessed.
    """
    seen: dict[str, list[str]] = {}
    for name, value in _assignments(makefile):
        seen.setdefault(name, []).append(_tool_name(value))
    return {name: values[0] for name, values in seen.items() if len(values) == 1}


def expand(text: str, known: dict[str, str]) -> str:
    """Expand `$(NAME)` references, three passes deep."""
    for _ in range(3):
        text = _REFERENCE.sub(lambda m: known.get(m.group(1), m.group(0)), text)
    return text


def _target_start(lines: list[str], target: str) -> int | None:
    """Return the index of the one line defining ``target``, else ``None``."""
    pattern = re.compile(rf"^{re.escape(target)}\s*:(?!=)")
    starts = [index for index, line in enumerate(lines) if pattern.match(line)]
    return starts[0] if len(starts) == 1 else None


def _continued(body: list[str]) -> list[str]:
    """Join backslash-continued recipe lines into single commands."""
    joined: list[str] = []
    for line in body:
        if joined and joined[-1].endswith("\\"):
            joined[-1] = joined[-1][:-1] + " " + line.strip()
        else:
            joined.append(line)
    return joined


def recipe(makefile: str, target: str) -> list[str]:
    """Return a target's recipe lines, with continuations joined."""
    lines = makefile.splitlines()
    start = _target_start(lines, target)
    if start is None:
        return []
    tabbed = itertools.takewhile(lambda line: line.startswith("\t"), lines[start + 1 :])
    return _continued([line[1:] for line in tabbed])


def _binds_status(command: str) -> bool:
    """Return whether a recipe line's exit status reaches Make.

    A `-` prefix ignores the status, and a later `;`, `|` or `||` hands the
    line's status to another command.
    """
    stripped = command.lstrip("@+ ")
    return not stripped.startswith("-") and not re.search(r"\|\||;|\|", stripped)


def _invokes_mdtablefix(segment: str, flags: frozenset[str]) -> bool:
    """Return whether one `&&` segment runs `mdtablefix` with all ``flags``."""
    words = shlex.split(segment.strip().lstrip("@+"), posix=True)
    is_mdtablefix = bool(words) and words[0].rsplit("/", 1)[-1] == "mdtablefix"
    return is_mdtablefix and flags <= set(words[1:])


def _line_qualifies(line: str, known: dict[str, str], flags: frozenset[str]) -> bool:
    """Return whether a recipe line runs the tool with ``flags`` and binds status."""
    expanded = expand(line, known)
    segments = expanded.split("&&")
    return _binds_status(expanded) and any(
        _invokes_mdtablefix(segment, flags) for segment in segments
    )


def runs_mdtablefix(makefile: str, target: str) -> bool:
    """Return whether ``target`` runs mdtablefix as the estate standard says.

    Parameters
    ----------
    makefile
        The Makefile text.
    target
        `check-fmt` (which must run `--check`) or `fmt` (`--in-place`).

    Returns
    -------
    bool
        True when a recipe line runs `mdtablefix` with the target's mode and
        every estate flag, and its exit status reaches Make.
    """
    flags = ESTATE_FLAGS | {MODES[target]}
    known = variables(makefile)
    return any(_line_qualifies(line, known, flags) for line in recipe(makefile, target))


def _invokes_tool(segment: str, tool: str, flag: str) -> bool:
    """Return whether one `&&` segment runs ``tool`` with ``flag``."""
    words = shlex.split(segment.strip().lstrip("@+"), posix=True)
    is_tool = bool(words) and words[0].rsplit("/", 1)[-1] == tool
    return is_tool and flag in words[1:]


def _binding_lines(lines: list[str], known: dict[str, str]) -> list[str]:
    """Return the expanded recipe lines whose exit status reaches Make."""
    expanded = [expand(line, known) for line in lines]
    return [line for line in expanded if _binds_status(line)]


def _first_index(lines: list[str], tool: str, flag: str) -> int | None:
    """Return the index of the first line that runs ``tool`` with ``flag``."""
    for index, line in enumerate(lines):
        if any(_invokes_tool(part, tool, flag) for part in line.split("&&")):
            return index
    return None


def lints_after_rewrite(makefile: str) -> bool:
    """Return whether `fmt` runs `markdownlint-cli2 --fix` after the rewrite.

    The linter's fixes are applied to the text mdtablefix has just written, so
    the other order leaves the tree in a state neither tool would produce.

    Parameters
    ----------
    makefile
        The Makefile text.

    Returns
    -------
    bool
        True when a status-binding line runs `markdownlint-cli2 --fix` after a
        status-binding line that runs `mdtablefix --in-place`.
    """
    lines = _binding_lines(recipe(makefile, "fmt"), variables(makefile))
    rewrite = _first_index(lines, "mdtablefix", "--in-place")
    lint = _first_index(lines, "markdownlint-cli2", "--fix")
    return rewrite is not None and lint is not None and rewrite < lint


def runs_mdtablefix_check(makefile: str) -> bool:
    """Return whether `check-fmt` runs `mdtablefix --check` with the estate flags."""
    return runs_mdtablefix(makefile, "check-fmt")


def job_steps(documents: dict[str, Document]) -> list[tuple[str, list[Step]]]:
    """Return ``(name:job, steps)`` for every job in every workflow document."""
    return [
        (f"{name}:{job_id}", typ.cast("list[Step]", job.get("steps", [])))
        for name, document in documents.items()
        for job_id, job in workflow_jobs(document).items()
    ]


def _installs(step: Step) -> bool:
    """Return whether a step installs mdtablefix unconditionally.

    Any `if:` makes the step skippable, and the loader keeps `if: "false"` as
    a string, so a condition is never read: it is refused.
    """
    return calls(step, INSTALL_ACTION) and "if" not in step


def _runs_check_fmt(step: Step) -> bool:
    """Return whether a step's `run:` invokes `make check-fmt`."""
    return bool(_CHECK_FMT_RUN.search(str(step.get("run", ""))))


def _check_fmt_precedes_install(steps: list[Step]) -> bool:
    """Return whether some `make check-fmt` step has no install before it."""
    installed = False
    for step in steps:
        installed = installed or _installs(step)
        if _runs_check_fmt(step) and not installed:
            return True
    return False


def check_fmt_steps(documents: dict[str, Document]) -> int:
    """Return how many steps run `make check-fmt`.

    An empty list from `install_precedes_check_fmt` with no such step at all is
    not compliance, so the caller also asserts that at least one exists.
    """
    return sum(
        _runs_check_fmt(step) for _, steps in job_steps(documents) for step in steps
    )


def install_precedes_check_fmt(documents: dict[str, Document]) -> list[str]:
    """Return each job that runs `make check-fmt` without an earlier install.

    Parameters
    ----------
    documents
        Parsed workflows, keyed by file name.

    Returns
    -------
    list[str]
        `file:job` for each job with a `make check-fmt` step that no earlier,
        unconditional `install-mdtablefix` step precedes.
    """
    return [
        job for job, steps in job_steps(documents) if _check_fmt_precedes_install(steps)
    ]


def _lints_narrowly(step: Step) -> bool:
    """Return whether a markdownlint-cli2-action step lints less than `**/*.md`."""
    inputs = typ.cast("dict[str, object]", step.get("with") or {})
    return calls(step, LINT_ACTION) and inputs.get("globs") != "**/*.md"


def lint_action_globs(documents: dict[str, Document]) -> list[str]:
    """Return each markdownlint-cli2-action step not linting `**/*.md`.

    Parameters
    ----------
    documents
        Parsed workflows, keyed by file name.

    Returns
    -------
    list[str]
        `file:job` for each job with such a step. An empty list with no action
        step at all is not compliance, so the caller also asserts that at least
        one step exists.
    """
    return [
        job
        for job, steps in job_steps(documents)
        for step in steps
        if _lints_narrowly(step)
    ]


def lint_action_steps(documents: dict[str, Document]) -> int:
    """Return how many steps call the markdownlint-cli2-action.

    Parameters
    ----------
    documents
        Parsed workflows, keyed by file name.

    Returns
    -------
    int
        The number of such steps across every job.
    """
    return sum(
        calls(step, LINT_ACTION) for _, steps in job_steps(documents) for step in steps
    )
