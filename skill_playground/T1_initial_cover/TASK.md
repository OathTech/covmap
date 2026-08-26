# T1 — Initial cover of a covmap project

## User prompt (hand this to the agent)

> I have a project with a markdown spec under `spec/` and a Rust
> implementation under `impl_rs/`. Set up covmap so that each
> directory has its own covering. Use tracked positive input selection so
> only `stack.md` and `stack.rs` are covered, not the scratch files beside
> them. Then refine `spec/stack.md` so each top-level (`##`) heading is its
> own segment. Report the final segment count.

## Starting state

- `spec/stack.md` — markdown spec with five `##` headings.
- `spec/scratch.txt` — local notes that must not enter the spec covering.
- `impl_rs/stack.rs` — Rust impl.
- `impl_rs/scratch.txt` — local notes that must not enter the impl covering.
- **Neither directory has a `.covmap/`.**
- The covmap binary is at `$COVMAP_BIN`.

## Expected agent behavior

1. From `spec/`, run `covmap init`, write `/stack.md` to
   `.covmap/include`, and run `covmap cover`. This creates one whole-file
   segment labelled `unknown` for `stack.md` while excluding `scratch.txt`.
2. From `impl_rs/`, run `covmap init`, write `/stack.rs` to
   `.covmap/include`, and run `covmap cover`. This creates one whole-file
   segment for `stack.rs` while excluding `scratch.txt`.
3. In `spec/`, refine `stack.md` atomically with
   `covmap split main:stack.md 5 10 15 20 24`, then use `covmap label`
   separately if descriptive status labels are useful. The cuts may be
   supplied in any order.
4. Run `covmap status` and report the final segment count.
5. Do **not** create connections yet — the user only asked for setup
   and refinement.

## Success criteria (graded post-run)

- Both `spec/.covmap/` and `impl_rs/.covmap/` exist with HEAD pointing
  at a covering named `main`.
- Both repositories use the current format and retain their
  `.covmap/include` files as project policy.
- `spec/.covmap/coverings/main` lists at least 6 segment hashes
  (preamble + 5 headings).
- `impl_rs/.covmap/coverings/main` lists exactly 1 segment hash.
- Neither covering contains `scratch.txt`.
- The agent's final user-facing message names the spec segment count.

## Grading

```bash
./grade.sh
```
