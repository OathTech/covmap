# covmap — Requirements

High-level requirements for covmap. Each captures a capability the tool
provides, derived from the formal model (`theory/main.tex`) and the
command interface (`covmap --help`). Intentionally coarse: the list stayed
at 20 for a long time, and the count is a deliberate ceiling, lifted only
when a genuinely new capability lands — req21 (iterators) and req22 (segment
messages) are such lifts. All are realized today except req11 (connection
algebra), which is explicitly kept as a recommended future direction.

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

## req3 — Two-level segment identity
Every segment shall have a hash identity: the 64-bit truncated hash
of its content, so identical content yields a single shared object. A
segment may additionally carry a stable, covering-local *name*, unique
within its covering, giving it a name identity that is independent of its
bytes. The name is not part of the segment hash, so naming, relabelling,
or recutting a segment leaves the shared object store untouched.

## req4 — Labels as ref-local status metadata
A segment's label shall be covering-local metadata that records
per-segment status (unknown, ignored, reviewed, mapped) and shall **not**
participate in the segment hash. Identity is a pure function of content,
so identical content is one shared object regardless of label, a segment
may carry different labels in different coverings, and re-labelling never
changes a hash or breaks a hash-addressed connection endpoint. Labels thus
join names (req3) and messages (req22) as non-hashed annotations.

## req5 — Coverings are partitions
A covering shall partition each covered file into disjoint, complete,
contiguous, file-confined segments, and this invariant shall be checkable.

## req6 — Partial, declared coverage
A covering need not span the whole repository; it shall explicitly declare
the set of files it accepts responsibility for.

## req7 — Additive, idempotent cover
covmap shall seed a covering with one whole-file segment per readable
file — the *initial cover* — and re-running it shall add only new files,
never disturbing existing refinements.

## req8 — Manual refinement only
covmap shall ship no automatic segmentation rule; coverings are refined
solely by explicit split, merge, recut, label, and name operations.
Names are assigned deliberately by the user or agent (never
auto-generated), since their value is a stable, human-meaningful
identity for the segments being mapped.

## req9 — Multiple coverings with automatic sharing
A repository shall support several coverings at once, sharing identical
segments automatically by content address rather than by duplication.

## req10 — Many-to-many annotated connections
Connections shall be many-to-many relations between coverings, with
optional per-link notes, supporting fan-out and fan-in between segments.
Each endpoint shall reference its segment either by name (preferred
for mapped segments, so the link is resilient to content edits) or by
segment hash.

## req11 — Connection algebra (recommended future direction)
The model equips connections with inverse, composition, and union
(`theory/main.tex`, Operations), so derived relations can be computed
from existing ones. Unlike the other requirements, this one is *not yet
realized*: the algebra is specified in the theory but has no CLI surface.
It is kept as a recommended future direction, to be implemented once a
concrete workflow needs it (e.g. composing a docs→code map with a
code→test map to derive docs→test).

## req12 — Quantitative measures
covmap shall report covering metrics (cardinality, line count,
granularity) and connection coverage (source and target totality).
Partition entropy is a defined covering measure (`theory/main.tex`) but
is computed only, not surfaced in command output — it is not actionable
on its own.

## req13 — Drift detection and layered repair
covmap shall detect when edited files diverge from their recorded
segments, and shall offer a layered repair path: recut for segment
drift (bytes changed, boundaries intact); diff-remap for boundary drift
(lines added/removed), which re-fits surviving segments by diffing the
reconstructed file against disk and carrying boundaries through
unchanged regions; and, for a segment that moved or was rewritten beyond
remapping, a propose-only fingerprint suggestion of its new location
ranked by similarity. Fingerprinting shall never mutate state on its
own. merge then split remains available as the manual fallback.

## req14 — Broken-link detection and pruning
covmap shall identify connection links whose endpoints no longer
resolve in their coverings and offer to drop (prune) them. An endpoint resolves
by name (broken only when the named segment is deleted) or by hash
(broken when that content no longer appears), so a content edit to a
named segment does not, by itself, break a link.

## req15 — Integrity verification
covmap shall verify on demand that every covering is a valid partition
and that every connection endpoint resolves.

## req16 — Stable, git-style addressing
Objects shall be addressable by hash, by covering-qualified hash, by
covering-qualified segment name (`<C>:@<name>`), by covering-qualified
position (`<C>:<file>:<line>[:<col>]`, resolving to the unique segment
containing that position — a stable, hash-free handle), or by connection
name, with HEAD resolving the default covering and unique hash prefixes
accepted.

## req17 — Plain-text, versioned state on disk
All state shall live under a `.covmap/` directory as a content-addressed
object store plus plain-text covering and connection refs, carrying a
format version so mismatched repositories are rejected.

## req18 — Tracked-file control via ignore
covmap shall honor gitignore-lite patterns in `.covmap/ignore` so users
keep build output, vendored code, and generated files out of coverings.

## req19 — Composable primitives (plumbing / porcelain)
covmap shall expose a small set of lower-level *plumbing* commands
emitting machine-readable, line-oriented output, from which every
higher-level *porcelain* command is intended to be expressible as a
pipeline. (This is git's plumbing/porcelain split: primitives plus the
convenience commands built over them.)

## req20 — Inspection, search, and visualization
covmap shall let users inspect, search, and visualize coverings and
connections (status, show, ls, grep, diff, and a static HTML browser).
Inspection output shall resolve link endpoints to their covering, label,
and location — not leave them as bare hashes — and `status` shall report
per-connection source/target coverage (req12), so a reader sees a mapping's
completeness and its relations without hand-joining separate outputs.

## req21 — Coverings as consumable work plans (iterators)
covmap shall freeze a covering — optionally filtered to a single label —
into an *iterator*: an ordered, stateful worklist whose tasks a consumer
(an agent, a human, a script) pulls one at a time, works against, and
marks `done` or `skipped`. An iterator is a snapshot layered over the
covering: it records a per-task state and a cursor but never mutates
segments, coverings, or connections, and — because the object store is
append-only — each frozen task's content stays retrievable even after the
covering is edited or deleted. It shall guarantee **complete enumeration**:
`next` advances to the first pending task in the frozen order and is
refused while the current task is still pending, so no task is stepped over
unresolved and iteration is exhausted exactly when every task is `done` or
`skipped`. Iterators live under `.covmap/iterators/`, a purely additive
directory that needs no format-version bump.

## req22 — Segment messages (annotation)
covmap shall let a segment carry an optional, free-form *message*: durable,
possibly multi-line text recording why the segment was cut, what it
represents, or why it is (not) mapped. A message is covering-local metadata,
distinct from the segment's `label` (a status category) and its
`name` (a slug identity, req3): like both of those it is **not** part of the
segment hash, so
setting, changing, or clearing it leaves the shared object store untouched
and never causes or repairs drift. It travels with its segment across
`split`/`merge`/`recut`/diff-remap the way a name does, is set and cleared
with `covmap annotate` and surfaced by `show`/`ls`/`html`, and is stored
additively in the covering ref with no format-version bump.

## req23 — Bounded drill-down to understanding
covmap shall let a reader reach *understanding of one region of a mapping*
— a set of source segments together with the segments they map to — in a
short, bounded sequence of inspection commands, without unbounded dumps or
manual cross-referencing. Concretely: inspection shall support restricting a
listing to a **region** (a covered file, or a line range within it) and to a
**label**; a `show` of a region or label shall present each selected segment
with its resolved image (req20), realizing κ(S) (the image of a subset under
a connection); and a `show` of a single segment or link shall resolve its
links in both directions. A large connection shall summarize (coverage plus
the highest fan-in targets) rather than dump every link unless `--all` is
given. This is output/query behavior over existing state; it needs no
format-version bump.
