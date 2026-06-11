# covmap

Coverings and connections between repositories.

covmap partitions a repository into content-addressed **segments**
grouped into named **coverings**, then records **connections**
between segments of two coverings. Coverings and connections are
independent on-disk artifacts, organized like `git` — there is an
object store, refs, and a `HEAD`.

There are no segmentation rules. `covmap cover` walks the repo and
assigns one whole-file segment labelled `unknown` to each readable
UTF-8 file. The user (or an agent) refines the covering with
`covmap split`, `covmap merge`, and `covmap recut`.

## Layout

```
covmap/
  Cargo.toml
  src/                # Rust implementation
    main.rs cli.rs repo.rs segment.rs covering.rs connection.rs
    walk.rs store.rs html.rs
  theory/main.tex     # mathematical foundation (run pdflatex)
  smoke_test/         # end-to-end shell test
    spec/ impl_rs/ run.sh
  skill_playground/   # self-contained agent tasks
    T1_bootstrap T2_link T3_drift_repair
    run_task.sh
```

## Build

```
cargo build --release
```

The binary is at `target/release/covmap`.

## Quick start

```
cd /path/to/spec
covmap init
covmap cover               # one whole-file `unknown` segment per file
covmap status              # cardinality, entropy, drift

# Refine: split spec.md at line 10, labelling top "Intro" and bottom "Body"
covmap ls -l               # find the spec.md hash
covmap split main:<hash> 10 "Intro" "Body"

cd /path/to/impl
covmap init
covmap cover

cd /path/to/spec
covmap connect main /path/to/impl:main spec_to_impl
covmap link spec_to_impl <hashA> <hashB> "fn foo"
```

After a spec edit, `covmap status` reports drift; use `covmap recut`
(boundaries unchanged) or `covmap merge` + `covmap split` (boundaries
moved) to refresh, then re-link any broken pairs.

See `theory/main.tex` for the mathematical model and
[`SKILL.md`](SKILL.md) for the agent-facing workflow guide. (Claude
Code finds the skill via a symlink at
`.claude/skills/covmap/SKILL.md` → `covmap/SKILL.md`.)

## Smoke test

```
./smoke_test/run.sh
```

Goes through init → cover → split → merge → connect → link → fsck
→ label → drift → repair on a small fixture in a temporary
directory.

## Skill playground

```
./skill_playground/run_task.sh T1_bootstrap
```

Instantiates a self-contained agent task under
`skill_playground/_runs/<task>/<ts>/`. Each task has a `TASK.md`
prompt and a `grade.sh` script that scores whatever the agent does.
