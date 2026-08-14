# Developing covmap

This document is for anyone — human or agent — changing covmap. It
describes the one rule that governs how the project evolves: **the six
artifacts must stay consistent with each other.** Read the *Consistency
contract* before you touch anything.

## The six artifacts

covmap is described in six places. The first five move from formal model
to practical use; the glossary is the cross-cutting vocabulary index. Each
is a lens on the same tool, and a change is not finished until all six agree.

| # | Artifact | Files | Role |
|---|----------|-------|------|
| a | **Theory** | `theory/main.tex` | The formal model — definitions, invariants, propositions. The ground truth of *what covmap means*. |
| b | **Requirements** | `REQUIREMENTS.md` | Coarse capabilities the implementation must provide, each derived from and mapped to the theory (via the `reqmap` connection in this repo's own `.covmap/`). |
| c | **Implementation** | `src/` (incl. `#[cfg(test)]` unit tests) | The Rust CLI and its unit tests. *What covmap does.* |
| d | **Skill** | `SKILL.md` + `skill_playground/` | The agent-facing workflow guide and its fixture-based evaluation tasks (T1–T8). *How to drive covmap.* |
| e | **Tutorial** | `tutorial/TUTORIAL.md` + `tutorial/run_tutorial.sh` | A guided end-to-end tour; the script is the source of truth and doubles as an integration test. *How covmap feels in practice.* |
| f | **Glossary** | `GLOSSARY.md` | The shared vocabulary across the other artifacts, with cross-references to primary definitions. *What covmap's terms mean consistently.* |

## Consistency contract

> **A change to any one artifact must be reconciled across all six
> before it lands.**

The artifacts form a dependency order — **a → b → c → d → e → f**.
Semantic changes can originate in any of `a`–`e` (a bug fix starts in `c`;
a new idea may start in `a`), while `f` is downstream-only: it records
vocabulary settled by the preceding artifacts rather than defining it.
Wherever a change starts, propagate it in that order so the downstream
layers rest on a settled definition.

### Propagation by change type

**New capability**
1. **Theory** — add/extend the definition, invariant, or proposition.
2. **Requirements** — add or amend a `req`, and map it to the theory
   block via `reqmap`.
3. **Implementation** — implement it; add unit tests covering the new
   behavior and its edge cases.
4. **Skill** — document the workflow in `SKILL.md`; add or extend a
   `skill_playground/` task if it introduces an agent-visible operation.
5. **Tutorial** — add a step to `run_tutorial.sh` (with an assertion) and
   narrate it in `TUTORIAL.md`.
6. **Glossary** — add or amend entries for the capability's concepts and
   cross-reference their primary definitions.

**Behavior change** (existing capability behaves differently)
- Reconcile the theory wording, the affected `req`, the code + unit
  tests, any `SKILL.md`/playground step that assumed the old behavior,
  the tutorial narration + assertions, and affected glossary entries.
  Re-run every gate below.

**Rename / vocabulary change**
- Update the term across all six artifacts and `README.md`. Check CLI
  help strings (`src/cli.rs`), on-disk ref
  comments, and playground `grade.sh`/`setup.sh` fixtures for the old
  name. Search the whole tree: `git grep -n '<old-term>'`.

**Bug fix (no semantic change)**
- Usually `c` only. If the bug existed because the theory or a `req` was
  ambiguous, fix the ambiguity there too so it does not recur.

**On-disk format change**
- Bump `FORMAT_VERSION` in `src/repo.rs`, update the format description
  in `README.md` and `theory/main.tex` §5, and confirm `req17`
  (versioned state) still holds.

## Gates

Everything CI enforces (`.gitlab-ci.yml`), runnable locally:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo test --all          # unit tests (artifact c)
./smoke_test/run.sh       # end-to-end shell test
```

Not yet in CI but part of the contract — run them by hand when your
change touches the skill or tutorial:

```sh
./tutorial/run_tutorial.sh                 # integration test for artifact e
./skill_playground/run_task.sh T1_initial_cover   # (and the other tasks you touched)
```

Self-consistency of the theory↔requirements map lives in this repo's own
`.covmap/`; after editing `REQUIREMENTS.md` or `theory/main.tex`, verify
it:

```sh
covmap status            # drift on the `main` / `theory` coverings
covmap fsck              # partitions valid, every reqmap endpoint resolves
```

## Definition of done

A change is complete when:

- [ ] All six artifacts reflect it (or you have consciously decided one
      is out of scope — say so in the commit message).
- [ ] `README.md` reflects any vocabulary/format change.
- [ ] All gates above pass, including the by-hand ones relevant to the
      change.
- [ ] `covmap fsck` on this repo's own `.covmap/` is clean.
