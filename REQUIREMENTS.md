# covmap — Requirements

High-level requirements for covmap. Each captures a capability the tool
provides, derived from the formal model (`theory/main.tex`) and the
command interface (`covmap --help`). Intentionally coarse: the list stayed
at 20 for a long time, and the count is a deliberate ceiling, lifted only
for a genuinely distinct capability. req21 through req24 are such lifts.
All are realized today except req11 (connection algebra), which is explicitly
kept as a recommended future direction. req24 is realized by on-disk format 3
and CIP 0010.

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
docs-to-code links within one tree. A connection shall have an explicit
lifecycle: creating it records an initially empty relation, while deleting it
removes only that connection ref and leaves coverings, segment objects, and
derived reports untouched.

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
join names (req3) and messages (req22) as non-hashed annotations. Partition
geometry and status metadata shall be composed explicitly: `split` accepts
only cut positions, and every fragment created by subdivision starts
`unknown` until separately relabelled.

## req5 — Coverings are partitions
A covering shall partition each covered file into disjoint, complete,
contiguous, file-confined segments, and this invariant shall be checkable.
Inserting any finite set of valid interior cuts shall preserve the partition;
the complete plan shall validate before the covering ref is replaced once.

## req6 — Partial, declared coverage
A covering need not span the whole repository; it shall explicitly declare
the set of files it accepts responsibility for.

## req7 — Additive, idempotent cover
covmap shall seed a covering with one whole-file segment per readable
file — the *initial cover* — and re-running it shall add only new files,
never disturbing existing refinements.

## req8 — Manual refinement only
covmap shall ship no automatic segmentation rule; coverings are refined
solely by explicit split, merge, recut, label, name, and annotate operations.
`split` shall atomically insert one or more explicit line or character cut
positions into one original segment or across one covered file, without
accepting label, name, or message operands. `merge` remains the explicit
operation that removes a cut. Refinement shall use recorded segment bytes;
when their cache object is absent it may reconstruct those bytes only from a
checked-out range whose content reproduces the recorded hash.
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
segment hash. Renaming or clearing a segment name shall remain a
covering-local operation: covmap shall not rewrite connection refs
implicitly, and shall instead diagnose discoverable exact `@old` endpoints
and offer an explicit repair using ordinary link and unlink operations when
that repair is safe. Subdivision shall likewise diagnose discoverable local
connection effects without rewriting refs: a name endpoint continues to
resolve but narrows to the first fragment, while a hash endpoint to the
original segment becomes broken.

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
granularity), connection coverage (source and target totality), and resolved
connection degree (maximum in/out degree plus bounded non-trivial fan-in and
fan-out hubs). Degree shall be keyed by resolved segment identity rather than
stored endpoint spelling: covering entries with the same content hash form one
operational node and one hub row. Links with either endpoint broken shall not
contribute.
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
by name (broken when the name is deleted, cleared, or renamed) or by hash
(broken when that content no longer appears), so a content edit to a
named segment does not, by itself, break a link. Rename diagnostics shall
report affected connection refs stored in the current repository without
claiming to discover inbound refs stored elsewhere. Subdivision diagnostics
shall make the same discovery limit explicit.

## req15 — Integrity verification
covmap shall verify on demand that every covering is a valid partition
and that every connection endpoint resolves. Verification shall compare live
source ranges with their recorded hashes and validate every cache object that
is present. A missing live cache object is not corruption when source
reconstructs it exactly; `hydrate` shall atomically restore such objects
without changing refs and shall reject unavailable or drifted source.
Unavailable covered source is an integrity failure even when a valid object
remains cached: the live source and covering ref together define the current
project artifact. Offline verification shall check local state and
self-connections while reporting external targets as unchecked, with an option
to hydrate reconstructible objects during the pass. The same partition
validator shall reject an invalid multi-cut plan before its covering ref is
replaced. Verification shall reject malformed peer bindings, unfinished peer
renames, invalid peer aliases, and external aliases that resolve back to the
current repository instead of using the canonical self peer `.`.

## req16 — Stable, git-style addressing
Objects shall be addressable by hash, by covering-qualified hash, by
covering-qualified segment name (`<C>:@<name>`), by covering-qualified
position (`<C>:<file>:<line>[:<col>]`, resolving to the unique segment
containing that position — a stable, hash-free handle), or by connection
name, with HEAD resolving the default covering and unique hash prefixes
accepted. A bare hash prefix shall resolve over the union of hashes in every
local covering ref and retained cache objects, deduplicating repeated
references to the same full hash; it shall therefore remain usable for a clean
live segment when the cache is empty. A covering-qualified hash shall constrain
resolution to that covering. Within `link` and `unlink`, each endpoint operand
shall accept a name, hash prefix, or position resolved against that side's
covering.
For `split`, an exact colon-free covered-file path shall outrank hash-prefix
interpretation and permit cuts across that file; a selector containing `:`
shall use positional-address syntax, and a bare file selector shall use HEAD.

## req17 — Plain-text, versioned state on disk
All state shall live under a `.covmap/` directory as a content-addressed
object cache plus plain-text covering and connection refs, carrying a format
version so mismatched repositories are rejected. Format 3 shall separate
portable project artifacts from checkout-local state and store connection
targets as peer aliases rather than paths. For live covering entries, the ref
and matching checked-out source are sufficient to reconstruct a missing
object; an existing object whose bytes do not hash to its filename is corrupt.

## req18 — Tracked-file control via ignore
covmap shall honor gitignore-lite patterns in `.covmap/ignore` so users
keep build output, vendored code, and generated files out of coverings.

## req19 — Composable primitives (plumbing / porcelain)
covmap shall expose a small set of lower-level *plumbing* commands
emitting machine-readable, line-oriented output, from which every
higher-level *porcelain* command is intended to be expressible as a
pipeline. (This is git's plumbing/porcelain split: primitives plus the
convenience commands built over them.) Multi-cut split shall compose the
existing position and object primitives while making one atomic covering-ref
transition rather than exposing intermediate partitions. `rev-parse` and
`cat-segment` shall use the same logical local hash index as porcelain readers,
so cache population does not change whether a clean live hash resolves.
Covering rows, connection rows, and peer bindings shall serialize in canonical
order so repeated writes of equal logical state produce equal bytes.

## req20 — Inspection, search, and visualization
covmap shall let users inspect, search, and visualize coverings and
connections (status, show, ls, grep, diff, and a static HTML browser).
Inspection output shall resolve link endpoints to their covering, label,
and location — not leave them as bare hashes — and `status` shall report
the path of each edited or unreadable covered file plus per-connection
source/target coverage (req12), so a reader sees a mapping's integrity,
completeness, and relations without hand-joining separate outputs.
Connections whose source and target repositories are the same shall be
displayed explicitly as self-maps rather than as an absolute-path target.
Connection link listings shall place an existing single-line note on wrapped,
indented lines below its resolved endpoint pair while leaving
`ls-links --resolve` unchanged. Inspection that reads live segment bytes shall
reconstruct them from matching source when absent and may publish them to the
cache on a best-effort basis. `status`, `show`, and `ls` shall opportunistically
warm selected live objects without failing solely because the cache is
read-only; source drift, unavailable bytes, and cache corruption shall remain
visible. `grep`, `cat-segment`, and HTML generation shall use the same checked
source-backed read behavior. Verification shall remain read-only unless
hydration is explicitly requested. When an external peer is unavailable,
read-only inspection shall preserve the local source-side result and mark the
target unchecked rather than misclassifying every target endpoint as broken;
commands that need target bytes or mutate the relation shall fail with binding
recovery guidance.

## req21 — Coverings as consumable work plans (iterators)
covmap shall freeze a covering — optionally filtered to a single label —
into an *iterator*: an ordered, stateful worklist whose tasks a consumer
(an agent, a human, a script) pulls one at a time, works against, and
marks `done` or `skipped`. An iterator is a snapshot layered over the
covering: it records a per-task state and a cursor but never mutates
segments, coverings, or connections, and — because the object store is
append-only — each frozen task's content stays retrievable even after the
covering is edited or deleted. Iterator creation shall ensure every frozen
task object is present; unlike a live covering entry, a deleted covering
cannot later reconstruct that task from source coordinates. It shall
guarantee **complete enumeration**:
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
and never causes or repairs drift. On subdivision, the first fragment keeps
the original message and later fragments have none; merge/recut/diff-remap
otherwise carry it with the segment the way a name does. It is set and
cleared with `covmap annotate`, surfaced by `show`/`ls`/`html`, and stored
additively in the covering ref with no format-version bump.

## req23 — Bounded drill-down to understanding
covmap shall let a reader reach *understanding of one region of a mapping*
— a set of source segments together with the segments they map to — in a
short, bounded sequence of inspection commands, without unbounded dumps or
manual cross-referencing. Concretely: inspection shall support restricting a
listing to the union of zero or more **regions** (covered files or line ranges)
and intersecting that union with a **label** or link-status filter; a `show` of
a region or label shall present each selected segment with its resolved image
(req20), realizing κ(S) (the image of a subset under a connection); and a
`show` of a single segment or link shall resolve its links in both directions.
A connection shall report degree maxima and at most ten non-trivial fan-in and
fan-out hubs per direction, while the existing large-connection threshold
continues to suppress the individual link list unless `--all` is given. This
is output/query behavior over existing state; it needs no format-version bump.

## req24 — Git-trackable coverings and mappings
covmap shall make coverings, connections, and project configuration portable,
deterministic, source-verifiable plain-text artifacts suitable for ordinary
Git tracking, review, branching, merging, and recovery. Equal logical state
shall serialize to equal bytes, and durable refs shall not contain
machine-specific checkout paths. An external connection shall name a stable
peer alias whose checkout path is supplied by ignored local binding state.

The default publication policy shall make ordinary `git add .covmap` include
covering refs, connection refs, and project metadata while excluding the
reconstructible object cache, peer bindings, iterator work state, generated
reports, and temporary files. Checked-out source plus the tracked artifact
shall be sufficient for offline verification and hydration of every clean live
segment. Iterator snapshots remain local unless a team deliberately publishes
both the worklist and its retained objects.

Git is the publication mechanism, not a runtime dependency: covmap shall not
invoke Git, inspect its index, stage files, or commit changes, and the same
artifact semantics shall work outside a Git repository. `covmap init` shall
create a commented repository-scoped default ignore policy and report the
durable/local boundary. `covmap migrate` shall upgrade format 1 or 2
repositories to format 3 through resumable staging, preserving a user-managed
publication policy while warning when it omits required local-state patterns.
