# Atomic multi-cut refinement

Stage: **Accepted**

## Relationship to other CIPs

This is the sole selected refinement proposal. It keeps the existing complete
covering model and v2 format.

[CIP 0007](0007-connection-lifecycle-workflows.md) owns connection lifecycle,
including diagnostics and repair hints when a covering-local rename leaves
stored endpoints stale. This proposal owns only connection effects caused by
subdivision itself.

[CIP 0006](0006-mapping-review-surfaces.md) is independent and continues to
inspect ordinary covering segments.

## Idea

Keep a covering as a complete partition of every covered file. Generalize
`split` from one cut in one segment to an atomic list of cut positions:

```text
covmap split <C>:<segment-address> <cut> [<cut> ...]
covmap split <C>:<file>            <cut> [<cut> ...]
```

The command inserts every requested cut into the existing partition, writes
the covering ref once, and takes no label, name, or message operands. Geometry
is changed by `split`; metadata is composed afterward with `label`, `name`, and
`annotate`.

No `segment` or `unsegment` verb is added. Exact-range isolation is two cuts;
coarsening remains the explicit `merge` operation.

## Motivation

Refining a large segment at several declarations or headings currently needs a
sequence of `split` commands. Each command changes hashes and writes the
covering ref, forcing the caller to resolve the next segment again.

The common information available while reading source is the start of each
new region. A cut list states those starts directly:

```sh
covmap split main:src/worker.rs 40 75 110
```

A range-oriented verb would repeat the end implied by the next start for this
common workflow. It helps mainly when isolating one interior or character-level
clause, where two cuts are still a short composition. Current evidence does
not justify a second refinement verb.

Removing label operands also eliminates the parser collision between
position-shaped labels and additional cuts. It reinforces the existing model:
labels are covering-local status metadata, not partition geometry.

## Model fit

For each covered file, a covering remains a complete ordered partition. It is
equivalently described by its interior cut set. Multi-cut split performs one
set insertion:

```text
cuts'(f) = cuts(f) union requested-cuts(f)
```

It cannot create a gap or overlap. Concatenating all replacement fragments for
an affected segment reproduces that segment's stored content exactly, so
concatenating the complete covering still reconstructs the recorded file for
drift detection and `recut --remap`.

The following existing invariants remain unchanged:

1. every covered-file position belongs to exactly one segment;
2. segment identity is the content-only hash;
3. labels, names, and messages are covering-local metadata;
4. refinement reads stored object content, not potentially drifted disk bytes;
5. `merge` is the explicit operation that removes a cut.

No on-disk representation or repository format version changes.

## Scope and attention

Complete geometry does not require every segment to make the same epistemic
claim. Existing metadata already distinguishes useful states:

- `unknown`: not yet examined;
- `ignored`: examined and outside the current mapping concern;
- `reviewed` but unlinked: examined, with no counterpart recorded;
- `mapped` or linked: a positive mapping claim;
- message text: the rationale for any of those decisions.

Iterator `done` and `skipped` states are deliberately separate. They are
verdicts in one frozen worklist and do not mutate the covering, so they must not
be treated as persistent covering labels or connection-coverage inputs.

This proposal does not change the current connection-coverage denominator,
drift reporting, or reserve any label string in the file format. A
status-aware denominator, including the exact treatment of `unknown`,
`ignored`, and linked-but-ignored segments, is a separate follow-up because it
changes req12 semantics rather than refinement.

## Command surface

```text
covmap split <C>:<segment-address> <cut> [<cut> ...]
covmap split <C>:<file>            <cut> [<cut> ...]
```

`<cut>` uses the existing 1-indexed `line[:column]` grammar and denotes the
first character of the fragment below the cut. A bare line is its first
character.

The selector forms behave as follows:

- A segment address may be a hash prefix, `@name`, or positional address. Every
  cut must lie strictly inside that one original segment.
- A file selector permits cuts in one or more original segments of that file.
  Each cut is resolved against the covering state present when the command
  starts.
- A bare file selector uses `HEAD`, consistent with other covering-addressed
  commands.

All remaining operands are cuts. The command accepts no label operands and has
no `--` parsing mode.

### Selector classification

After resolving the optional covering prefix, classify the selector token in
this order:

| Selector token | Meaning |
| --- | --- |
| `@name` | segment selected by exact name |
| token containing `:` (`file:line[:column]`) | positional segment address |
| exact colon-free path in `covered_files()` | whole-file selector |
| otherwise | segment selected by unique hash prefix |

Thus:

```text
split main:src/worker.rs:40 75
```

means one cut at 75, confined to the segment containing line 40, while:

```text
split main:src/worker.rs 40 75
```

means two file-level cuts.

An exact colon-free covered-file path wins over hash-prefix interpretation.
This follows the general rule that an exact semantic name outranks a prefix
match. In the rare case of a hex-like file path that is also a valid hash
prefix, use a longer hash prefix, `@name`, or a positional address to select
the segment.

As in the current resolver, any `:` in the selector remainder reserves
positional-address syntax. A malformed suffix produces a positional-address
error rather than falling through to file or hash lookup. Repository paths
containing `:` are therefore not file selectors; this proposal does not add an
escaping syntax.

### Validation and atomicity

Before changing the covering ref:

1. parse and canonicalize every cut;
2. sort cuts in file position order;
3. reject duplicate operands, file start/end cuts, malformed positions, and
   cuts outside the selected segment or file;
4. for a file selector, recognize an existing interior boundary as an
   already-satisfied no-op;
5. group new cuts by the original segment that contains them;
6. read every affected object and validate every resulting fragment.

Only after the complete plan validates does the command write new objects and
replace the covering ref once. A validation or object-write failure leaves the
ref unchanged. Append-only objects written before a later failure may be
unreachable, which is harmless.

For a file selector, if every requested cut already exists, return success
without rewriting the ref. Repeating a successful file-addressed command is
therefore idempotent. A segment selector remains scoped to that one original
segment; after it is subdivided, the same hash no longer resolves and a
position/name selector may denote only one result.

Output groups each affected original segment with its resulting hashes and
locations in file order, and separately reports already-existing cuts.

### Metadata propagation

When an original segment is actually subdivided:

- every resulting fragment receives label `unknown`, conservatively
  invalidating status at the new granularity;
- the first fragment, which contains the original start, keeps the original
  name and message; and
- later fragments are anonymous and un-messaged.

This is the current default split behavior generalized to any number of cuts.
The output identifies the fragment that inherited a name or message.

Metadata is then applied explicitly:

```sh
covmap label    main:src/worker.rs:40 reviewed
covmap name     main:src/worker.rs:40 validate
covmap annotate main:src/worker.rs:40 -m "Validates the normalized request."
```

The current positional split labels are removed from the public grammar. This
is a pre-1.0 breaking change, but it avoids mixing status updates into the
partition operation and leaves one unambiguous variadic operand type.

Every operand after the selector is now interpreted as a cut. An alphabetic
former label fails position parsing and can receive a metadata-composition
hint. A former label that itself has valid position syntax, such as `42`,
cannot be distinguished from an intended cut and is treated as a cut. Release
notes and migration guidance must call out this incompatibility explicitly;
the implementation must not pretend it can diagnose the user's old intent.

### Connection diagnostics

Before writing, inspect connections stored in the current repository for
endpoints affected by each subdivided segment:

- A local `@name` endpoint continues to resolve to the first fragment, but its
  denotation narrows. Warn with the connection, side, old range, and new range.
- A local hash endpoint naming the old segment becomes broken. Warn with the
  connection and side.
- An unlinked segment produces no connection warning.

Inbound connections stored in another repository cannot be discovered. When
one or more warnings are emitted, one accompanying note states that limit.

Moving or dropping the inherited name remains explicit composition with
`name`. Changing the name token and repairing stale stored endpoints is
specified by CIP 0007.

## Why no `segment` or `unsegment`

For a complete partition, an exact inclusive range `[start,end]` is isolated by
inserting `start` and `successor(end)` when those are interior cuts. This is
composition over the same primitive, not a distinct state transition.

The observed refinement workflow is dominated by known starts: headings,
declarations, and sequential steps. A separate range verb would make that
common case longer and would be used mainly for occasional interior
character-range isolation. It does not currently clear the frequency bar for
permanent convenience surface.

`unsegment` has no meaning without gaps. To stop treating a segment as active
scope, relabel or annotate it. To remove an unnecessary boundary, use `merge`.

If later usage shows exact interior ranges are a constant operation rather
than an occasional composition, a convenience command can be reconsidered on
frequency evidence.

## Merge

`merge` remains unchanged by this proposal, including its current label
operand and name/message propagation behavior. Improving merge conflict
handling is separable and must not be smuggled into the multi-cut change.

Re-segmenting across an existing boundary remains explicit:

1. reconcile any metadata or endpoint consequences;
2. `merge` the adjacent segments; and
3. `split` at the desired cut positions.

## Surface-area check

This proposal adds no verb or flag. It generalizes one existing operation from
one cut to many cuts and removes its optional metadata operands.

The repeated split/resolve/write loop cannot be removed by diagnostics,
rendering, documentation, or shell composition without retaining multiple
intermediate ref states. Atomic multi-cut is therefore a genuine
generalization rather than convenience duplication.

## Implementation

- Add a split-specific selector classifier with the precedence table above.
  Reuse `covered_files()` for exact file recognition and the existing
  name/hash/position resolvers after classification.
- Parse all remaining operands with the existing position parser.
- Add a covering-level `split_many` planner operating over a cloned covering.
- Reuse the current Unicode character-offset and boundary helpers; do not
  duplicate content slicing.
- Group cuts by original segment, slice each stored object once, and build all
  replacement entries before the single ref write.
- Extract the complete-partition checks currently embedded in `fsck` into a
  reusable validator.
- Reuse connection endpoint resolution for pre-write subdivision warnings.
- Remove positional label parsing from `cmd_split`.
- Update both general and focused CLI help/usage strings to the new variadic,
  label-free grammar.
- Keep covering refs, objects, connections, and repository format v2
  unchanged.

## Tests

### Parsing and selection

- One cut through a hash, name, or position selector retains current geometric
  and propagation behavior when labels were omitted.
- The selector grammar distinguishes `C:file:line cut` from
  `C:file cut...` exactly as specified.
- An exact covered-file path wins over a colliding hash prefix; a non-file
  token continues through unique hash-prefix resolution.
- A malformed positional suffix fails as an address rather than silently
  falling through to a different selector kind.
- A file selector cuts one original segment at several positions.
- A file selector may atomically cut several original segments.
- Line and character cuts share the existing position grammar.
- Unsorted cuts are applied in file order.
- Duplicate, malformed, outer-boundary, out-of-file, and out-of-selected-
  segment cuts fail without changing the ref.
- File-addressed existing interior cuts are reported as no-ops; an
  all-existing request does not rewrite the ref.
- Alphabetic former label operands fail with a metadata-composition hint.
- Position-shaped former labels are treated as cuts and are documented as a
  breaking grammar migration.

### Partition and atomicity

- Multi-cut output matches repeated default-label single splits in hashes,
  ranges, labels, names, and messages.
- Replacement fragments concatenate to the original object byte-for-byte.
- The final covering remains complete, disjoint, confined, and ordered.
- The covering ref is written once after full validation.
- Object-write failure leaves the covering ref unchanged.
- Repeating a successful file-addressed request is idempotent.

### Metadata and connections

- All new fragments are `unknown`.
- The first fragment keeps the original name/message; later fragments do not.
- Named subdivision warns with the narrowed range only when a discoverable
  name endpoint exists.
- Hash endpoints to subdivided segments are reported as becoming broken.
- Diagnostics state the foreign-connection discovery limit.

## Deferred

- Status-aware connection coverage and the treatment of `unknown`, `ignored`,
  and linked-but-ignored segments.
- A range convenience verb, pending evidence that exact interior isolation is
  a frequent operation.
- Merge metadata/endpoint conflict policy.

## Landing checklist

This is a behavior and CLI grammar change. Per `DEVELOPER.md`, land it in
artifact order:

- **Theory:** generalize split from one cut to finite cut-set insertion; retain
  complete partitions and explicit merge; separate refinement geometry from
  status metadata.
- **Requirements:** amend req4, req5, req8, req10, req14-req16, req19, and
  req22 for label-free atomic multi-cut and subdivision diagnostics.
- **Implementation:** add the planner, atomic ref update, warnings, and
  unit/integration tests; update the general and focused CLI usage strings.
- **Skill:** replace the old inline-label signature and every label-bearing
  split example with file-addressed multi-cut followed by explicit metadata
  commands.
- **Tutorial:** update both `tutorial/TUTORIAL.md` and
  `tutorial/run_tutorial.sh`; batch the heading refinement and specifically
  rewrite the named character-level split so its labels are applied by
  separate metadata commands.
- **README / glossary:** update split signatures, the README worked example,
  metadata composition, and the complete-covering rationale.
- **Smoke tests:** exercise multi-cut atomicity, idempotence, and unchanged v2
  state.
- **Self-map:** remap every amended requirement and run `covmap status` plus
  `covmap fsck`.
