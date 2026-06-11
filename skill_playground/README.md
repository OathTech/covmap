# covmap skill playground

Self-contained tasks for evaluating how well an agent drives covmap
via the [covmap skill](../../.claude/skills/covmap/SKILL.md).

## Tasks

- **T1_bootstrap** — initialize covmap on two unconfigured directories.
- **T2_link** — bootstrap is done; link spec headings to an
  implementation segment.
- **T3_drift_repair** — a spec edit broke one connection pair; recover
  the covering and re-link it.

## Running a task

```bash
./run_task.sh T1_bootstrap
```

Each run instantiates a fresh workspace under `_runs/<task>/<ts>/`,
copies the fixture, runs `setup.sh` if present, and prints the
workspace path. Hand `TASK.md` to an agent with the workspace as its
cwd; once the agent finishes, run `./grade.sh` from the workspace to
score the result.

The covmap binary is built once into `covmap/target/release/covmap`
and is referenced by every task via `$COVMAP_BIN`.
