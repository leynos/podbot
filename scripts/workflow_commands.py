"""Which steps run a given command, and what could stop them running.

Split from `workflow_contracts`, which reads the documents, so that
neither module passes the 400-line limit `AGENTS.md` sets. A contract that
requires a lane to run a command asks this module, and gets back each
matching step with the guards of its step and its job.
"""

from __future__ import annotations

import typing as typ

from workflow_contracts import of_type, parse  # noqa: F401 - `parse` serves the doctests


class CommandStep(typ.NamedTuple):
    """One step running a given command, with the guards around it.

    Attributes
    ----------
    job : str
        The owning job's name. Kept because a guard on the job disables the
        step as surely as a guard on the step, and a collection that drops
        the owner cannot see it.
    job_guard : object
        The job's `if:`, or `None`.
    step_guard : object
        The step's `if:`, or `None`.
    """

    job: str
    job_guard: object
    step_guard: object

    @property
    def can_run(self) -> bool:
        """Report whether nothing guards this step.

        Returns
        -------
        bool
            True when neither the step nor its job declares an `if:`.

        Examples
        --------
        >>> CommandStep("lint", None, None).can_run
        True
        >>> CommandStep("lint", False, None).can_run
        False
        """
        return self.job_guard is None and self.step_guard is None

    def describe_guards(self) -> str:
        """Return the guards found, for a diagnostic.

        Returns
        -------
        str
            A description naming each guard, or "nothing" when unguarded.

        Examples
        --------
        >>> CommandStep("lint", False, "${{ false }}").describe_guards()
        "job if: False; step if: '${{ false }}'"
        >>> CommandStep("lint", None, None).describe_guards()
        'nothing'
        """
        parts = [
            f"{scope} if: {guard!r}"
            for scope, guard in (
                ("job", self.job_guard),
                ("step", self.step_guard),
            )
            if guard is not None
        ]
        return "; ".join(parts) if parts else "nothing"


def command_steps(document: dict[str, object], command: str) -> tuple[CommandStep, ...]:
    r"""Return every step whose `run:` is exactly ``command``.

    The comparison is equality on the stripped value rather than a
    substring search. A search is satisfied by ``echo 'make
    workflow-contracts'``, by the text inside a shell comment, and by any
    command that merely mentions it, so a contract written that way asserts
    that a string appears rather than that anything runs.

    Each match carries its owning job, because a step with no `if:` inside a
    job with ``if: false`` is dead code that an assertion about the step
    alone cannot see.

    Parameters
    ----------
    document : dict[str, object]
        A parsed workflow document.
    command : str
        The command the step must run, exactly.

    Returns
    -------
    tuple[CommandStep, ...]
        One entry per matching step.

    Examples
    --------
    >>> text = (
    ...     "jobs:\n  lint:\n    if: false\n    steps:\n"
    ...     "      - run: make workflow-contracts\n"
    ... )
    >>> found = command_steps(parse("ci.yml", text), "make workflow-contracts")
    >>> found[0].can_run, found[0].describe_guards()
    (False, 'job if: False')

    >>> text = "jobs:\n  lint:\n    steps:\n      - run: echo 'make x'\n"
    >>> command_steps(parse("ci.yml", text), "make x")
    ()
    """
    found: list[CommandStep] = []
    for name, job in of_type(document.get("jobs"), dict).items():
        job_map = of_type(job, dict)
        for step in of_type(job_map.get("steps"), list):
            step_map = of_type(step, dict)
            if str(step_map.get("run", "")).strip() != command:
                continue
            found.append(
                CommandStep(
                    job=str(name),
                    job_guard=job_map.get("if"),
                    step_guard=step_map.get("if"),
                )
            )
    return tuple(found)
