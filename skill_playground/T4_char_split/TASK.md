# T4 — Isolate an inline clause with a character-level split

## User prompt (hand this to the agent)

> covmap is already initialized in `spec/` and `impl_rs/`, and
> `spec/buffer.md` is split into three segments: a preamble, the
> `## Capacity` section, and the `## Push` section. An empty
> connection `buffer_links` from `spec/main` to `impl_rs/main` is set
> up.
>
> Inside the Capacity section, one sentence is the normative
> requirement that the implementation actually relies on: *"It must be
> a power of two so that the implementation can mask indices instead
> of taking a modulus."* Please make that exact sentence its own
> segment, then link it to the implementation segment that realizes
> it. Report status when done.

## Starting state

- `spec/.covmap/` and `impl_rs/.covmap/` populated.
- `spec/buffer.md` covering has 3 segments: preamble (1..4),
  Capacity (5..8), Push (9..11).
- The Capacity prose is a single long line (line 7) holding two
  sentences; only the second one is the normative requirement.
- `impl_rs/buffer.rs` is a single whole-file segment.
- `spec/.covmap/connections/buffer_links` exists but is empty.

## Expected agent behavior

1. Inspect the covering with `covmap ls -l` and read the Capacity
   segment (`covmap show main:<hash>`). Note that line 7 packs two
   sentences, so a whole-line split cannot separate them — a
   **character-level** boundary is required.
2. Find the column where the normative sentence begins on line 7
   (the `I` of "It must be a power of two…"). It is column 40.
3. Split at that character position:
   `covmap split main:<Capacity_hash> 7:40`, then label the two fragments
   separately with `covmap label`. The following fragment now begins mid-line
   at character 40.
4. If the new segment still trails into the blank line 8, split again
   at line 8 to leave the clause (`buffer.md:7:40..7`) on its own.
   Confirm with `covmap show` that the segment's content is exactly
   the normative sentence.
5. Link that clause segment to the single `impl_rs` segment:
   `covmap link buffer_links <clause_hash> <impl_hash> -m "power-of-two capacity"`.
6. Run `covmap status` and `covmap fsck`; confirm the covering is
   still a clean partition and the link is not broken.

## Notes on character positions

- Columns are 1-indexed and inclusive. In `covmap split`, the
  `<line>:<col>` argument names the **first character of the following
  fragment**; the preceding fragment ends at the character just before it.
- A range prints as `file:a..b`; a character column appears only when
  set, e.g. `buffer.md:7:40..7` (from column 40 of line 7 through the
  end of line 7).

## Success criteria (graded)

- `covmap fsck` passes in both `spec/` and `impl_rs/` — the covering
  is still a valid partition after the character-level split.
- Some segment of `buffer.md` carries a character column on line 7
  (proving a real character-level boundary was made).
- Exactly one segment's content equals the normative clause
  (ignoring trailing whitespace).
- That clause segment is linked to the `impl_rs` segment in
  `buffer_links`.

## Grading

```bash
./grade.sh
```
