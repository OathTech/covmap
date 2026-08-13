# Connection lifecycle and workflows

Stage: **Accepted**

## Relationship to CIP 0009

This proposal remains an active, independent implementation candidate and
should land before [CIP 0009](0009-atomic-multi-cut.md). It changes connection
refs, endpoint diagnostics, help, and presentation rather than covering
storage. CIP 0009 reuses its endpoint-discovery rules when subdivision affects
stored links.

## Idea

Complete the connection lifecycle, make same-repository connections explicit
in the interface, align help text with endpoint forms the implementation
already accepts, and make covering-local rename fallout accurately repairable.

## Motivation

The connection lifecycle is incomplete without a supported deletion
operation. Same-repository targets and name-addressed unlinking already work
but are obscured by help and inconsistent display. Renaming a segment is
covering-local and can leave stored `@name` endpoints stale; current output
suggests re-pointing without saying that no connection was changed.

## Current behavior

- `connect main main K` already means "target covering `main` in this
  repository", because a target without `:` defaults its repository to `.`.
  Help and the skill describe only `<repo>:<covering>`, so this shorthand is
  effectively hidden.
- `show <K>` marks same-repository connections as `(self-map)`, while `status`
  prints the absolute repository path followed by `/covering`.
- `unlink` already accepts `@name`, hash prefixes, and positional addresses,
  but CLI help, README, and theory still spell the operands as hashes.
- `name <C>:@old new` changes only the covering. Discoverable stored endpoints
  remain `@old`; `status` and `fsck` correctly report them broken, but the
  command's "re-point connections" wording is ambiguous and gives no exact
  repair.
- No command deletes a connection ref.
- Iterator `peek --all` already gives the full frozen worklist; its discovery
  can be improved in focused help without adding another alias.

## Command surface

### Same-repository target

Document and formalize:

```text
covmap connect <C_A> [<repo>:]<C_B> [<K>]
```

Examples:

```sh
covmap connect main main duplication
covmap connect main ../implementation:main spec_to_impl
```

The on-disk target repository remains canonical and absolute. Presentation
uses:

```text
duplication: main -> main (self-map)
```

instead of rendering the absolute repository path as though the covering were
a child directory.

Do not add a second `--self` spelling unless the bare-covering form proves
ambiguous in practice.

### Endpoint help

Use `<A>` and `<B>` consistently:

```text
covmap link   <K> <A> <B> [<note>]
covmap unlink <K> <A> <B>
```

Focused help must state that each operand may be `@name`, a hash prefix, or a
position in that side's covering. Add a regression test that creates and
removes the same link by `@name`.

### Delete a connection

Add:

```text
covmap disconnect <K>
```

The command:

1. requires exactly one existing connection name;
2. reads the ref first so malformed state is reported rather than silently
   deleted;
3. removes only `.covmap/connections/<K>`;
4. prints the number of links removed and the source/target coverings;
5. warns when `.covmap/report.html` exists that the derived report may be
   stale.

It must not delete arbitrary paths previously passed to `html -o`; covmap does
not track ownership of those exports. The default report can be regenerated or
removed explicitly by the user.

### Covering-local rename repair

Keep `name <C>:@old new` a covering-local operation. It must never mutate a
connection ref implicitly.

Before writing the renamed covering, discover connections stored in the
current repository whose relevant side uses that covering and whose endpoint
is exactly `@old`. After the covering write:

1. list each affected connection and side;
2. state that the stored endpoint is now broken;
3. when at least one affected local endpoint exists, state that inbound
   connections stored in another repository cannot be discovered; and
4. when safe, print the exact existing-command composition that repairs it.

For a rename whose far endpoint still resolves and whose replacement pair does
not already exist, print one delimited executable shell block for the whole
rename:

```text
--- covmap repair begin ---
covmap link 'K' '@new' '@target' '<existing note>'
covmap unlink 'K' '@old' '@target'
--- covmap repair end ---
```

Use symmetric operand order for a target-side rename. Add the replacement
before removing the stale pair: emit every safe `link` command before any
`unlink`, so a failed `link` leaves all old records intact. Preserve each
existing valid single-line note byte-for-byte, omit the note argument when
empty, and POSIX-shell-quote every dynamic argument.

If the far endpoint is broken or the replacement pair collides, report the
conflict without repair delimiters. Clearing a name has no generic replacement
endpoint; list affected links and the available `unlink` command, but do not
invent a relink target.

### Surface-area justification

`disconnect` is the only new top-level verb in this connection-lifecycle
proposal. Rename repair changes diagnostics only and composes existing `link`
and `unlink`. Existing deletion alternatives are insufficient:

- an error-message or display change cannot remove a connection ref;
- `unlink` removes one relation pair but leaves the connection itself;
- `prune` removes only broken pairs and also leaves the connection; and
- documentation currently has to recommend a forbidden manual edit.

Connection deletion is therefore the missing inverse of `connect`, not a
convenience alias. Its permanent cost is one additional verb for humans and
agents to learn. No new `--self`, endpoint, or iterator option is introduced
elsewhere in this proposal.

### Iterator discovery

Keep `peek --all`; do not add a synonymous `list` verb. Improve
`covmap iter <name>` / focused help to say:

```text
peek          list pending tasks
peek --all    list the complete frozen worklist with state and notes
```

## Implementation

- Adjust `connect` help and render self-map targets through one display helper
  shared by `status` and `show`.
- Add `Connection::remove` and `cmd_disconnect`; validate argument count and
  connection existence.
- Align link/unlink help strings and docs with the existing resolver.
- Add read-only affected-endpoint discovery, POSIX shell quoting, and
  delimited repair-block rendering to `name`; do not add an endpoint-mutation
  primitive.
- Keep the connection ref format unchanged.

## Tests

- `connect main main K` creates a self-map and both `status` and `show` label it
  unambiguously.
- `unlink K @source @target` removes a name-addressed link.
- `disconnect K` removes exactly the connection, leaves coverings/objects
  untouched, and makes subsequent `show K` fail clearly.
- A malformed connection ref is not deleted.
- A present default HTML report triggers the stale-report warning.
- Rename changes only the covering, accurately lists source- and target-side
  stale endpoints, and never says it re-pointed a connection.
- Executing emitted repair blocks restores resolution and preserves notes
  byte-for-byte.
- A fan-out rename emits one repair block with all `link` commands before all
  `unlink` commands, and that rendered block executes successfully.
- Shell quoting round-trips spaces, apostrophes, dollar signs, backticks, and
  other valid note characters without executing them.
- Broken far endpoints, replacement collisions, and name clearing produce no
  unsafe repair block.

## Landing checklist

- **Theory:** add connection deletion, state the same-repository target
  shorthand, and retain covering-local rename with explicit endpoint repair.
- **Requirements:** amend req2 (connection lifecycle/self-map), req16
  (endpoint operands), req10/req14 (rename fallout and broken endpoints), and
  req20 (display).
- **Implementation:** add `disconnect`, rename diagnostics/repair blocks,
  display/help fixes, and tests.
- **Skill:** document self-map syntax, deletion, rename repair, and iterator
  peek discovery; extend a playground task if `disconnect` is agent-visible.
- **Tutorial:** demonstrate a same-repo connection and delete a temporary
  connection; rename a linked endpoint, execute the emitted repair block, and
  assert its note is preserved.
- **README / glossary:** update command tables and self-map wording.
- **Self-map:** remap amended requirements and run `covmap status` plus
  `covmap fsck`.

No format-version bump is required.
