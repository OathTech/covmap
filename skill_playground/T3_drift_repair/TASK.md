# T3 — Repair drift after a spec edit

## User prompt (hand this to the agent)

> The cache spec was edited under `## Put` (an LRU clarification was
> appended). The file's bytes have changed but the line boundaries
> are intact. The implementation still satisfies the new spec —
> recover the connection so it is healthy again.

## Starting state

- `spec/.covmap/` and `impl_rs/.covmap/` populated.
- `spec/.covmap/connections/cache_links` has 3 pairs (Get, Put, Size).
- `spec/cache.md` was edited inside the Put segment. `covmap status`
  reports file-level drift, but no pairs are broken yet — the
  covering still holds the pre-edit Put segment.

## Expected agent behavior

1. From `spec/`, run `covmap status` and note the drift line.
2. Find the Put segment with `covmap ls -l` (or
   `covmap grep -l -c main "## Put"`).
3. Refresh the Put segment from current bytes:
   `covmap recut main:<old_put_hash>`. This keeps the same
   `(file, a, b, label)` and recomputes the hash. The Put pair in
   `cache_links` is now broken because the covering's Put hash
   changed.
4. Run `covmap fsck --broken-pairs` — it now lists the Put pair.
5. Re-link to the new hash:
   `covmap unlink cache_links <old_put_hash> <impl_hash>`
   `covmap link   cache_links <new_put_hash> <impl_hash> "Put (LRU)"`
6. Verify with `covmap status` that the connection has 3 pairs and
   0 broken, and `covmap fsck --broken-pairs` produces no output.

## Success criteria (graded)

- `cache_links` has exactly 3 pairs.
- `covmap fsck --broken-pairs` from `spec/` produces no output.
- `covmap status` from `spec/` shows `0 broken` for `cache_links`.

## Grading

```bash
./grade.sh
```
