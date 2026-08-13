# T10 — Repair a rename and delete a temporary connection

## User prompt (hand this to the agent)

> This repository has a same-repository connection named `audit`. Rename
> segment `@old-source` to `@current-source` without losing its link or note.
> Then create a temporary same-repository connection named `scratch`, inspect
> it, and delete it cleanly. Finish by reporting connection status and
> integrity.

## Starting state

- Covering `main` contains named segments `@old-source` and `@target`.
- Self-map `audit` contains one link from `@old-source` to `@target`.
- Its link note is `same-repo rationale`.

## Expected agent behavior

1. Inspect `audit` with `covmap status` or `covmap show audit`.
2. Run `covmap name main:@old-source current-source`.
3. Read the affected-endpoint diagnostics and execute the commands inside the
   emitted repair block. Do not edit `.covmap/` refs manually.
4. Confirm `audit` now links `@current-source` to `@target`, preserves the
   original note, and passes `covmap fsck`.
5. Run `covmap connect main main scratch`, inspect its `(self-map)` display,
   then run `covmap disconnect scratch`.
6. Report final `covmap status` and `covmap fsck`.

## Success criteria (graded)

- `audit` still exists with exactly one resolving link.
- The source endpoint is `@current-source`; `@old-source` is gone.
- The note remains byte-for-byte `same-repo rationale`.
- `scratch` no longer exists.
- The covering remains valid and `covmap fsck` passes.

## Grading

```bash
./grade.sh
```
