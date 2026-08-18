# T12 - Restore a Git-tracked mapping in a different checkout layout

## User prompt (hand this to the agent)

> The specification and implementation were cloned into `review/`, at paths
> different from the author's checkouts. The committed covmap connection must
> not be edited. From `review/spec-clone`, inspect the required peers, verify
> and hydrate the local covering offline, bind the existing `impl` alias to
> `../deps/implementation`, then run full integrity verification. Confirm that
> the connection still resolves and that Git remains clean.

## Starting state

- Both clones contain format-3 covering artifacts and matching source.
- `review/spec-clone/.covmap/connections/spec_to_impl` stores
  `target_peer=impl`, not an absolute checkout path.
- The clone has no `.covmap/objects/` content and no `.covmap/local/peers`.
- The connection bytes match the author's committed connection.

## Expected agent behavior

Run from `review/spec-clone`:

```sh
covmap peer ls
covmap fsck --offline --hydrate
covmap peer bind impl ../deps/implementation
covmap peer ls
covmap fsck
covmap show spec_to_impl
git status --short
```

The first `peer ls` reports `impl` as unbound. Offline verification checks the
local source endpoint and hydrates the clean live object while leaving the
external target unchecked. Binding supplies only checkout-local resolution;
it must not rewrite the committed connection.

## Success criteria (graded)

- The connection still has byte-identical `target_peer=impl` metadata.
- Full `covmap fsck` passes and `show` resolves the mapping.
- The local covering cache was hydrated.
- `.covmap/local/peers` resolves `impl` to the review implementation clone.
- Neither the cache nor peer binding is tracked, and `git status` is clean.

## Grading

```bash
./grade.sh
```
