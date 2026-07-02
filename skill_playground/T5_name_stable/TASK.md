# T5 — Name-stable links across an in-place edit

## User prompt (hand this to the agent)

> covmap is set up in `spec/` and `impl_rs/`. The spec covering refines
> `auth.md` into preamble / Login / Logout / Expiry. The Login section
> is a **named slot** `@login-contract`, linked to the implementation
> through the `auth_links` connection.
>
> I just edited the Login prose (tightened the failure-delay sentence).
> Refresh covmap so the covering is clean again, **without breaking the
> existing link**. Report status when done.

## Starting state

- `spec/.covmap/` and `impl_rs/.covmap/` populated.
- `spec/auth.md` covering: preamble (1..4), Login (5..9), Logout (10..14),
  Expiry (15..18). The Login slot is named `@login-contract`.
- `auth_links` connects `spec/main` to `impl_rs/main` with one link
  whose source endpoint is `@login-contract` (a name, not a hash).
- `spec/auth.md` has been edited inside the Login section. The edit did
  not add or remove lines, so the slot's boundaries are intact.

## Expected agent behavior

1. Run `covmap status`. Note the drift line (one file edited) — but also
   note that `auth_links` already reports **0 broken**: the link is by
   name, so a byte change inside the slot does not orphan it. This is the
   key difference from a hash-based link (compare T3).
2. Refresh the slot's recorded content with a single `covmap recut`
   on the Login slot (find it with `covmap ls -l`, e.g. the row tagged
   `@login-contract`). Because the boundaries did not move, `recut`
   succeeds.
3. Do **not** unlink/relink: the connection references `@login-contract`,
   whose name is unchanged, so the link stays valid across the recut.
4. Run `covmap status` again: drift is now clean and `auth_links` still
   shows 1 link, 0 broken.

## Why this matters

With a hash-based link, editing bytes inside a mapped segment changes
its hash and breaks the link (the agent must unlink the old hash and
link the new one — see T3). A **name-tag** decouples link identity from
content: the slot keeps its name across edits, so only a `recut` to
refresh the segment hash is needed, and the connection never breaks.

## Success criteria (graded)

- The Login slot is still named `@login-contract`.
- `auth_links` has exactly one link, referencing `@login-contract` (by
  name, not a bare hash).
- After the recut, `covmap status` shows `drift: clean` and
  `auth_links … 1 links, 0 broken`.
- `covmap fsck` reports no broken links and passes.

## Grading

```bash
./grade.sh
```
