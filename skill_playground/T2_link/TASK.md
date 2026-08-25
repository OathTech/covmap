# T2 — Link spec headings to the implementation

## User prompt (hand this to the agent)

> covmap is already initialized in `spec/` and `impl_rs/`. The spec
> covering `main` has been refined so each `##` heading (Enqueue,
> Dequeue, Length) is its own segment. An empty connection
> `queue_links` from `spec/main` to `impl_rs/main` is set up.
> Please link each `##` heading segment to the implementation
> segment that realizes it, then report status.

## Starting state

- `spec/.covmap/` and `impl_rs/.covmap/` populated.
- `spec/queue.md` covering has 4 segments: a preamble plus Enqueue,
  Dequeue, Length.
- `impl_rs/queue.rs` is a single whole-file segment.
- `spec/.covmap/connections/queue_links` exists but is empty.

## Expected agent behavior

1. From `spec/`, list segments: `covmap ls -l`. Identify the three
   heading segments (Enqueue, Dequeue, Length) by their labels.
2. From `impl_rs/`, list segments: `covmap ls-segments`. There is
   exactly one impl segment.
3. Add three links to `queue_links` with `covmap link queue_links
   <spec_hash> <impl_hash> -m "<note>"` — one for each heading. The
   note should mention the corresponding Rust function. Give at least one
   link a second `-m` paragraph recording how you validated the mapping.
4. Run `covmap status` from `spec/` and confirm `0 broken` and
   `3 links`.

## Success criteria (graded)

- `queue_links` has exactly 3 links.
- All three links reference the single impl_rs hash on the right.
- Every link has a note, and at least one note has two paragraphs.
- `covmap fsck` passes in both directories.

## Grading

```bash
./grade.sh
```
