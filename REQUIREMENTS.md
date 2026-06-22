# covmap — Requirements

High-level requirements for covmap. Each captures a capability the tool
must provide, derived from the formal model (`theory/main.tex`) and the
command interface (`covmap --help`). At most 20, intentionally coarse.

Each requirement is mapped — via the `reqmap` connection in this repo's
own `.covmap/` — to the theory block that justifies it. Open the HTML
report and use the Compare view to navigate requirement ↔ justification.

## req1 — Cover a single repository
covmap shall partition a repository into segments, forming a covering —
the basic unit on which all mapping and analysis is built.

## req2 — Relate coverings via connections
covmap shall link two coverings by recording connections between their
segments. The two coverings may belong to different repositories or to
the same repository — even the same covering — enabling both cross-repo
mapping and intra-project cross-references such as duplication audits or
docs-to-code links within one tree.

## req3 — Content-addressed segment identity
Every segment shall be identified by the 64-bit truncated hash of its
labelled content, so identical labelled content yields a single identity.

## req4 — Labels as part of identity
A segment's label shall participate in its identity, letting per-segment
status (unknown, ignored, reviewed, mapped) coexist with content sharing.

## req5 — Coverings are partitions
A covering shall partition each covered file into disjoint, complete,
contiguous, file-confined blocks, and this invariant shall be checkable.

## req6 — Partial, declared coverage
A covering need not span the whole repository; it shall explicitly declare
the set of files it accepts responsibility for.

## req7 — Additive, idempotent bootstrap
covmap shall seed a covering with one whole-file segment per readable
file, and re-running it shall add only new files, never disturbing
existing refinements.

## req8 — Manual refinement only
covmap shall ship no automatic segmentation rule; coverings are refined
solely by explicit split, merge, recut, and label operations.

## req9 — Multiple coverings with automatic sharing
A repository shall support several coverings at once, sharing identical
segments automatically by content address rather than by duplication.

## req10 — Many-to-many annotated connections
Connections shall be many-to-many relations between coverings, with
optional per-pair notes, supporting fan-out and fan-in between segments.

## req11 — Connection algebra
Connections shall support inverse, composition, and union, so derived
relations can be computed from existing ones.

## req12 — Quantitative measures
covmap shall report covering metrics (cardinality, granularity, partition
entropy) and connection coverage (source and target totality).

## req13 — Drift detection and guided repair
covmap shall detect when edited files diverge from their recorded
segments and distinguish file-level drift (repaired by recut) from
boundary drift (repaired by merge then split).

## req14 — Broken-pair detection and repair
covmap shall identify connection pairs whose endpoints no longer exist in
their coverings and offer to drop them.

## req15 — Integrity verification
covmap shall verify on demand that every covering is a valid partition
and that every connection endpoint resolves.

## req16 — Stable, git-style addressing
Objects shall be addressable by hash, by covering-qualified hash, or by
connection name, with HEAD resolving the default covering and unique hash
prefixes accepted.

## req17 — Plain-text, versioned state on disk
All state shall live under a `.covmap/` directory as a content-addressed
object store plus plain-text covering and connection refs, carrying a
format version so mismatched repositories are rejected.

## req18 — Tracked-file control via ignore
covmap shall honor gitignore-lite patterns in `.covmap/ignore` so users
keep build output, vendored code, and generated files out of coverings.

## req19 — Composable plumbing
Every porcelain command shall be expressible as a pipeline of lower-level
plumbing commands emitting machine-readable, line-oriented output.

## req20 — Inspection, search, and visualization
covmap shall let users inspect, search, and visualize coverings and
connections (status, show, ls, grep, diff, and a static HTML browser).
