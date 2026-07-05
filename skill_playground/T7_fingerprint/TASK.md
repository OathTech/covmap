# T7 — Relocate a moved segment from a fingerprint proposal

## User prompt (hand this to the agent)

> covmap is set up in `spec/` and `impl_rs/`. `spec/policy.md` is split
> into preamble / Audit / User / Backups; the Audit logs section is a
> named segment `@audit-retention` linked to the implementation through
> `policy_links`.
>
> I reorganized the doc and moved the whole "Audit logs" section to the
> bottom of the file. Figure out where that section went and get the
> covering back in sync, keeping the link to the implementation. Report
> status when done.

## Starting state

- `spec/.covmap/` and `impl_rs/.covmap/` populated.
- Before the edit: preamble (1..4), Audit (5..8), User (9..12),
  Backups (13..15). `@audit-retention` is linked via `policy_links`.
- `spec/policy.md` was edited: the "Audit logs" section was **moved**
  from the middle to the end. Its old boundary lines no longer exist
  where they were, so a diff cannot place it by position.

## Expected agent behavior

1. Run `covmap status`: one file edited.
2. Try `covmap recut --remap`. Diff-remap cannot resolve the moved segment
   (its boundary lines were deleted from the middle), so it leaves the
   file untouched and prints a **fingerprint proposal** — a ranked,
   scored guess of where the segment's content now lives, e.g.

   ```
   @audit-retention: policy.md:13..15  (NN% match)  — covmap unlink/link to relocate
   ```

   Nothing is changed by this step: fingerprinting is propose-only.
3. **Confirm** the proposal by reading those lines
   (`covmap show` / open the file) and checking they are indeed the
   Audit logs section.
4. Re-cut the covering so the Audit logs section is a segment again at its
   new position (e.g. `merge` the disturbed region and `split` at the
   new heading lines, or split the relevant segment), then `name` that
   segment `@audit-retention` and ensure `policy_links` points at it.
5. Run `covmap status` / `covmap fsck`: clean partition, link intact.

## Why this matters

Diff-remap (T6) handles edits that keep boundaries in unchanged regions.
A **moved** segment defeats it — but the segment's content still exists
somewhere in the file. Fingerprinting finds the most similar location
and proposes it, turning "the link is broken, hunt for where it went"
into "covmap thinks it's here (NN%), confirm." The proposal is advisory:
covmap never relinks on a fuzzy match by itself.

## Success criteria (graded)

The grader runs `covmap recut --remap` itself and checks that:

- it reports an **unresolved** segment and prints a fingerprint proposal
  naming `@audit-retention`, pointing at the moved segment, with a match
  percentage;
- the covering ref was **not mutated** by the remap (propose-only);
- the `@audit-retention` segment and its `policy_links` link are still
  present (nothing was dropped).

## Grading

```bash
./grade.sh
```
