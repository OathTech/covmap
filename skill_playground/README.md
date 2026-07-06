# covmap skill playground

Self-contained tasks for evaluating how well an agent drives covmap
via the [covmap skill](../../.claude/skills/covmap/SKILL.md).

## Tasks

- **T1_initial_cover** — initialize covmap on two unconfigured
  directories and take the initial cover of each.
- **T2_link** — the initial cover is done; link spec headings to an
  implementation segment.
- **T3_drift_repair** — a spec edit broke one connection link; recover
  the covering and re-link it.
- **T4_char_split** — isolate one inline clause with a character-level
  split (`split <C>:<h> <line>:<col>`), then link it.
- **T5_name_stable** — a named, linked segment survives an in-place byte
  edit: `recut` refreshes it with no broken link and no relinking.
- **T6_diff_remap** — an insertion moves section boundaries;
  `recut --remap` re-fits every segment automatically, links preserved.
- **T7_fingerprint** — a section is moved elsewhere; `recut --remap`
  can't place it and prints a propose-only fingerprint relocation guess.
- **T8_iterator_sweep** — freeze a covering into an iterator and audit
  every segment exactly once, recording a per-task verdict; the
  enumeration guarantee ensures none is missed and the sweep is resumable.

## Running a task

```bash
./run_task.sh T1_initial_cover
```

Each run instantiates a fresh workspace under `_runs/<task>/<ts>/`,
copies the fixture, runs `setup.sh` if present, and prints the
workspace path. Hand `TASK.md` to an agent with the workspace as its
cwd; once the agent finishes, run `./grade.sh` from the workspace to
score the result.

The covmap binary is built once into `covmap/target/release/covmap`
and is referenced by every task via `$COVMAP_BIN`.
