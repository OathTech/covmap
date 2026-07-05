# T1 — Initial cover of a covmap project

## User prompt (hand this to the agent)

> I have a project with a markdown spec under `spec/` and a Rust
> implementation under `impl_rs/`. Set up covmap so that each
> directory has its own covering, then refine `spec/stack.md` so each
> top-level (`##`) heading is its own segment. Report the final
> segment count.

## Starting state

- `spec/stack.md` — markdown spec with five `##` headings.
- `impl_rs/stack.rs` — Rust impl.
- **Neither directory has a `.covmap/`.**
- The covmap binary is at `$COVMAP_BIN`.

## Expected agent behavior

1. From `spec/`, run `covmap init` and `covmap cover`. This creates
   one whole-file segment labelled `unknown` for `stack.md`.
2. From `impl_rs/`, run `covmap init` and `covmap cover`. Same: one
   whole-file segment for `stack.rs`.
3. In `spec/`, refine `stack.md` by repeatedly invoking
   `covmap split main:<hash> <line> <label_top> <label_bot>` at each
   `##` heading. The `##` headings are at lines 5, 10, 15, 20, 24.
4. Run `covmap status` and report the final segment count.
5. Do **not** create connections yet — the user only asked for setup
   and refinement.

## Success criteria (graded post-run)

- Both `spec/.covmap/` and `impl_rs/.covmap/` exist with HEAD pointing
  at a covering named `main`.
- `spec/.covmap/coverings/main` lists at least 6 segment hashes
  (preamble + 5 headings).
- `impl_rs/.covmap/coverings/main` lists exactly 1 segment hash.
- The agent's final user-facing message names the spec segment count.

## Grading

```bash
./grade.sh
```
