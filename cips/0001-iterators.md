# covmap iterators — coverings as consumable work plans

## Idea

A **covering** already partitions a project into labeled segments. An
**iterator** *freezes* a covering into an *ordered, stateful worklist*: each
segment becomes a **task** that an external consumer (an AI agent, a human, a
script) pulls one at a time, works against, and marks resolved. The covering
says *what the pieces are*; the iterator says *in what order to process them
and how far we've got*. This turns a static map into an executable plan.

The iterator is a **snapshot**, not a live view: `new` records the covering's
segments at freeze time, so enumeration is unaffected by later edits to the
files or the covering. Because covmap's object store is immortal, every frozen
task's content stays recoverable (`cat-segment`) no matter how the working tree
is later changed — the iterator is self-sufficient even if the covering is
deleted. Line numbers in a task are a *hint* to the current position; the frozen
content is the anchor.

The iterator guarantees a **complete enumeration**: every task is eventually
resolved (`done` or `skipped`), and you cannot skip past a task by accident —
see *Enumeration guarantee* below.

## Why it helps AI agents

Agents drift, repeat work, and lose the thread on large jobs. An iterator gives
them a durable cursor and a single in-scope chunk, so each step is small,
verifiable, and resumable — accuracy goes up because the plan lives on disk,
not in a context window. After a compaction or crash, `iter <name> current`
re-establishes exactly where the work stands.

## Model

An iterator is a thin layer over a covering; it never mutates segments or
coverings. Stored at `.covmap/iterators/<name>`:

```
# covering=<C>
# order=file|label
# label=<l>               optional — present only for a filtered freeze
# cursor=<hash>           the task the consumer is currently on (unset after new)
<hash>	<state>	<note>    state: pending|done|skipped; note optional
...
```

- **Freeze contents.** `new` snapshots one line per included segment, recording
  its `(hash, file:a..b, label, name)`. The lines are stored in the chosen
  order; the object store makes the frozen content permanently retrievable.
- **States.** A task is `pending`, `done`, or `skipped`. There is no stored
  `active` state: the **cursor** (`# cursor=`) records the one task the consumer
  is on, and a task is *active* exactly when the cursor points at a `pending`
  task. After `new` the cursor is unset (before the first task).
- **Backward compatible, no `FORMAT_VERSION` bump.** `iterators/` is a new
  directory older covmap simply never reads, mirroring how the char-column and
  name fields were added additively.

## Commands

Grammar is **name-first**: `covmap iter <name> <verb>`, so `covmap iter <name>`
is a stable prefix. Bare `covmap iter` (no name) lists all iterators. There is
no implicit verb — `covmap iter <name>` with no verb is an error; ask for
`status` explicitly.

| Command | Purpose |
| ------- | ------- |
| `covmap iter` | List all iterators. |
| `covmap iter <name> new <C> [--order file\|label] [--label <l>]` | Freeze covering `C` into an ordered worklist. `--order` defaults to `file` (path, then line — natural reading order). `--label <l>` freezes only segments with label `<l>` (e.g. `unknown` for a backfill), narrowing the enumerated set. |
| `covmap iter <name> next` | Advance the cursor to the first `pending` task in covering order and print it. **Refused while the current task is still `pending`** (`done`/`skip` it first). Prints nothing and exits non-zero when no `pending` task remains. |
| `covmap iter <name> current` | Reprint the cursor's task in any state — the resume affordance. |
| `covmap iter <name> done [-m <msg>]` | Mark the cursor's task `done`. Optional note. Does not advance. |
| `covmap iter <name> skip [-m <msg>]` | Mark the cursor's task `skipped`. Optional note. Does not advance. |
| `covmap iter <name> choose <addr>` | Move the cursor to task `<addr>` (`@name` or hash). A deliberate jump; leaves every other task's state untouched. |
| `covmap iter <name> rewind [<addr>]` | Reset a `done`/`skipped` task to `pending` (default: the cursor's task). |
| `covmap iter <name> peek [--all] [--content]` | Look ahead at pending tasks without advancing (`--all` for the full remaining list). |
| `covmap iter <name> status` | `12/40 done, 1 active, 3 skipped`. |
| `covmap iter <name> finished` | Exit 0 iff every task is `done` or `skipped`. |
| `covmap iter <name> rm` | Delete this iterator (never touches the covering). |

### Print format

`next`, `current`, and `peek` print a one-line, parseable handle per task:

```
<@name|hash>	<label>	<file>:<a>..<b>
```

`--content` (on `next`, `current`, `peek`) additionally emits the frozen bytes,
so the loop's stdout stays terse by default and the consumer pulls content on
demand.

### Canonical agent loop

```sh
covmap iter sweep new main --order file
until covmap iter sweep finished; do
  covmap iter sweep next                 # first pending → active, printed
  # ...work on the current task...
  covmap iter sweep done -m "verdict"    # or: covmap iter sweep skip -m "false positive"
done
```

## Enumeration guarantee

The iterator guarantees that iteration visits every task and never silently
strands one:

- **`next` advances to the first `pending` task in covering order** — globally,
  not merely the next one after the cursor. So a task is never skipped over just
  because the cursor moved past it.
- **`next` is refused while the cursor's task is still `pending`.** To leave a
  task you must resolve it — `done` (worked it) or `skip` (deliberately not).
  This is what forbids the accidental "advance off an unresolved task"; the
  correct way to pass on a task is `skip` then `next`.
- **`choose` is the deliberate escape hatch.** Unlike `next`, it may move off a
  `pending` task — but that task stays `pending`, so a later `next` returns to
  it. Jumping around therefore cannot break completeness.
- **`finished` is true only when no `pending` task remains** — i.e. every task
  is `done` or `skipped`.

## Use cases

- **Port A→B**: cover project A, label chunks by concern, `iter A_port new main
  --order label`; each `next` is one unit to translate into B. Optionally `link`
  each finished task to its new B segment, so the iterator doubles as a coverage
  ledger.
- **Refactor sweep**: enumerate every call site / pattern as segments, iterate
  to apply the change one location at a time, `skip -m` the false positives.
  (Edits shift disk line numbers; rely on the frozen content, not the printed
  `file:a..b`, to relocate.)
- **Audit / review**: walk every segment exactly once for a security or license
  pass, `done -m "<verdict>"` recording the verdict per chunk in the note.
- **Test backfill**: `iter backfill new main --label unknown` to enumerate only
  the uncovered segments; each task = "write a test for this segment," `status`
  = how much of the backlog remains.

## Invariant

The iterator never mutates segments or coverings — it only records position and
per-task state over a frozen snapshot, so it composes cleanly with every
existing covmap operation.

## Landing checklist (six-artifact consistency)

Per `DEVELOPER.md`, this is a **new capability** and is not done until it is
reconciled across all six artifacts (a→b→c→d→e→f):

- **Theory** (`theory/main.tex`): define an iterator as a total order over a
  covering's (optionally label-filtered) segments plus a state map
  seg→{pending,done,skipped} and a cursor; state the *complete-enumeration*
  invariant alongside the C1–C3 partition invariants.
- **Requirements** (`REQUIREMENTS.md`): a new `req21`, mapped to the theory
  block via `reqmap`. Note the current "at most 20" framing must be lifted
  deliberately.
- **Implementation** (`src/`): new `src/iterator.rs` (struct + read/write via
  `store::atomic_write`, mirroring `covering.rs`/`connection.rs`); an
  `iterators_dir()` on `Repo`, created in `Repo::init`; a two-level `iter`
  dispatch arm in `src/cli.rs` (covmap's first multi-level command group — the
  dispatcher and `command_help_line`/`--help` today assume single-word
  commands); in-module `#[cfg(test)]` tests.
- **Skill** (`SKILL.md` + a `skill_playground/` task, next free is `T8`):
  document the loop and add a fixture task exercising it.
- **Tutorial** (`tutorial/`): a step in `run_tutorial.sh` with an assertion,
  narrated in `TUTORIAL.md`.
- **`GLOSSARY.md`**: entries for *iterator*, *task*, *cursor*, and the task
  states; no new drift vocabulary is introduced (freeze + immortal objects mean
  there is no `stale` task state).

No `FORMAT_VERSION` bump — `iterators/` is purely additive.
