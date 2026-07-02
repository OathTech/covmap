# AGENT.md

Guidance for AI agents working in this repository.

## Before changing anything: the consistency contract

covmap is described in five artifacts that must stay consistent with
each other: (a) theory `theory/main.tex`, (b) requirements
`REQUIREMENTS.md`, (c) implementation + unit tests `src/`, (d) skill
`SKILL.md` + `skill_playground/`, (e) tutorial `tutorial/`.

**A change to any one is not done until the other four (and
`GLOSSARY.md`) are reconciled.** Read [`DEVELOPER.md`](DEVELOPER.md) for
the propagation rules by change type, the gates to run, and the
definition of done. Do this before editing — the invariant is invisible
in any single file's diff.

## Working style

- `SKILL.md` is the guide to *driving* covmap as a tool; `DEVELOPER.md`
  is the guide to *changing* it. Use the right one for the task.
- Run the gates in `DEVELOPER.md` before considering a change complete,
  including `covmap fsck` on this repo's own `.covmap/`.
