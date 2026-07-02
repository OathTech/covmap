# T6 — Absorb a boundary-moving edit with diff-remap

## User prompt (hand this to the agent)

> covmap is set up in `spec/` and `impl_rs/`. `spec/api.md` is split
> into preamble / Create / Read / Delete; the Read and Delete sections
> are named slots (`@read-ep`, `@delete-ep`) linked to the
> implementation through `api_links`.
>
> I expanded the Create section by a couple of lines, which pushed Read
> and Delete down. Bring the covering back in sync with the file —
> without re-cutting every section by hand and without breaking the
> links. Report status when done.

## Starting state

- `spec/.covmap/` and `impl_rs/.covmap/` populated.
- Before the edit: preamble (1..4), Create (5..8), Read (9..12),
  Delete (13..15). `@read-ep` and `@delete-ep` are linked via
  `api_links`.
- `spec/api.md` was edited: two lines were inserted inside the Create
  section. Every section below Create has shifted down, so the recorded
  ranges for Read and Delete now point at the wrong lines.

## Expected agent behavior

1. Run `covmap status`: one file edited. Note the connection still
   reports 0 broken (the links are by name), but the recorded
   boundaries are stale.
2. This is **boundary drift**: the cut points moved. A per-segment
   `recut` is the wrong tool — it would re-hash whatever lines now sit
   in the old range, capturing the wrong text. Instead run:

   ```
   covmap recut --remap
   ```

   This reconstructs the pre-edit file from the covering, diffs it
   against the file on disk, and re-fits every slot's boundaries through
   the unchanged regions — automatically following the upstream
   insertion. Names (hence the `api_links` pairs) are preserved.
3. Run `covmap status` and `covmap fsck`: the covering is a clean
   partition again, and `api_links` still has 2 pairs, 0 broken.

## Why this matters

Before diff-remap, a boundary-moving edit meant manually `merge`-ing the
disturbed sections and re-`split`-ting at the new line numbers, then
re-linking. `recut --remap` does the boundary arithmetic for you: it is
deterministic for any edit that leaves the slot boundaries in unchanged
regions of the file (the common, localized case).

## Success criteria (graded)

- `covmap status` shows `drift: clean` and `api_links … 2 pairs, 0 broken`.
- `covmap fsck` passes with no broken pairs.
- The `@read-ep` slot's content is exactly the Read section after the
  shift (it followed the insertion rather than staying at the old lines).
- Reconstructing the file from the covering reproduces `api.md`
  byte-for-byte.

## Grading

```bash
./grade.sh
```
