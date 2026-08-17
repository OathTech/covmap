# covmap — Glossary

This glossary serves covmap's six-artifact consistency contract by keeping
terms aligned across theory, requirements, implementation, skill, and
tutorial. Its entries are drawn from and tagged by the three primary
technical sources. Definitions are kept short and cross-referenced; where
a term is defined precisely in the theory, the wording here is the informal
gloss.

The source tags are:
**[T]** theory · **[R]** requirements · **[S]** source. Entries are
alphabetical, except that a few umbrella concepts (**Drift**, **Repair**)
are immediately followed by their sub-concepts, book-index style, rather
than scattering them across the alphabet. Sub-concepts sit at the same
level as every other entry; the convention is simply that they come right
after their parent.

---

**Address** *(also: address form)* **[T][R][S]** — a stable, textual
reference to an object, in one of five forms, mixed freely:

- `<hash>` — a segment, by full or unique hex prefix;
- `<C>:<h>` — segment `h` in covering `C`;
- `<C>:@<name>` — a named segment, by its name in covering `C`;
- `<file>:<line>[:<col>]` — the segment of the covering that contains
  that position;
- `<K>` — a connection, referred to by its name alone (no covering
  prefix, no sigil), e.g. `reqmap`.

A `<h>` with no covering is short for `HEAD:<h>`; the literal `HEAD`
resolves to the default covering.

**Adjacency** **[T][S]** — two segments in the same file are *adjacent*
when the successor of the first's end position equals the second's
start. At line granularity this reduces to `b₁ + 1 = a₂`. Only adjacent
segments can be merged.

**Anonymous segment** **[T][S]** — a segment with no **segment name**.
Referable only by its segment hash, so any content edit invalidates
references to it. Contrast **named segment**.

**Atomic multi-cut** **[T][R][S]** — one `split` operation that inserts a
finite set of interior line or character boundaries into one segment or one
covered file. All cuts validate before any covering-ref change, and the ref is
replaced once. New fragments are labelled `unknown`; metadata is applied
separately. Repeating an all-existing file cut set is an idempotent no-op.

**Bounded drill-down** **[T][R][S]** — the intended inspection access
pattern (req23): understanding one **region** of a mapping takes a short,
bounded sequence of commands (orient → narrow → understand) whose final step
resolves the relation fully, so no follow-up lookup is forced. Listing
commands print one condensed line per item; a command naming one item prints
everything about it. Realized by `status` coverage, region/label selectors,
and **resolved link** output; see also the **HTML report / Compare view**.

**Cardinality** **[T][S]** — `|C|`, the number of segments in a covering.
A complexity measure reported by `status`.

**Cache corruption** **[T][R][S]** — a present object whose bytes do not
hash to the hash encoded by its `.covmap/objects/` path. Distinct from a
missing cache entry, which is valid for a live segment when checked-out source
reconstructs the recorded hash. Hydration never overwrites corruption; delete
the offending cache file, then hydrate it again from matching source.

**Connection** **[T][R][S]** — a many-to-many relation between two
coverings, stored as a set of links with optional per-link notes. This
is the "map" covmap produces. Stored separately from coverings, in the
source repo's `.covmap/connections/<name>`. The two coverings may be in
different repositories or the same one (even the same covering). `connect`
creates this ref; `disconnect` removes only this ref, not either covering,
the segment objects, or a derived report.

**Connection algebra** **[T][R]** — *(recommended future direction)* the
derived operations the model gives connections: *inverse* (`κ⁻¹`),
*composition* (`λ ∘ κ`), and *union*. Specified in the theory and
`req11`, but no CLI command exercises them today; kept as a documented
future direction, to be implemented once a concrete workflow needs it
(e.g. composing a docs→code map with a code→test map to derive
docs→test).

**Cover** **[T][R][S]** — the `covmap cover` operation that seeds a
covering with one **whole-file segment** (labelled `unknown`) per
readable UTF-8 file. Additive and idempotent: re-running adds only new
files and never disturbs existing refinements. covmap ships no automatic
segmentation rule, so this is the only non-manual step. Has one
sub-concept (below).

**Initial cover** **[T][R][S]** — the particular covering produced by the
first `cover` on a repository: one whole-file `unknown` segment per file,
before any **split**/**merge** refinement. Its **granularity** is near 0.
Sub-concept of **cover**.

**Coverage (source / target)** **[T][S]** — the fraction of a covering's
segments that appear in at least one link of a connection. A connection
is *source-total* when its source coverage is 1.

**Covered set** *(also: covered files)* **[T][R][S]** — the set `F` of
files a covering declares responsibility for partitioning. A covering
need not span the whole repository. On disk, the `# covered=` lines.

**Covering** **[T][R][S]** — a partition of each covered file into
disjoint, complete, contiguous, file-confined segments. The basic unit
on which all mapping and analysis is built. A repository may carry
several coverings at once.

**Covering invariants (C1–C3)** **[T]** — the three conditions a covering
must satisfy on each covered file: **disjointness** (no two segments
overlap), **completeness** (every character lies in some segment), and
**confinement** (every segment lies within a covered file). Checkable by
`fsck`.

**Cut / cut sequence** **[T][S]** — the ordered positions
`p₀ ≺ p₁ ≺ … ≺ p_k` delimiting a file's segments. `split` introduces a
new cut; `merge` removes one. Diff-remap carries interior cuts through an
edit via the line map.

**Degree (in / out)** **[T][R][S]** — for the resolving part of a connection,
the number of links entering or leaving one content-hash node. Hash and
`@name` spellings, and repeated covering entries with identical content, form
one node; its hub is listed once at the first covering-order location. Broken
links are excluded. A degree greater than one identifies a **fan-in** or
**fan-out hub**.

**Drift** **[T][R][S]** — divergence between a covering's recorded
segments and the current bytes of an edited file: the *repairable* state
that precedes **broken**. Detected without any rule by reconstructing
each file from the covering and comparing it to disk. Has three
sub-concepts (below): the first two distinguished by whether line
boundaries still fit, the third lifting drift from a segment to a link.

**Segment drift** **[T][R][S]** — bytes inside a segment changed but its
line boundaries still fit the file. Repaired by **recut**. Sub-concept of
**drift**.

**Boundary drift** **[T][R][S]** — an edit added or removed lines so a
segment's recorded range no longer fits the file (`b > |f|` on disk).
Repaired by **diff-remap**, or manually by **split**/**merge**.
Sub-concept of **drift**.

**Link drift** **[T][S]** — a link is *drifted* when an endpoint points at
a segment that has drifted (by name — the segment's content changed but it
still exists). The link still resolves, so it is not **broken**;
re-syncing the segment (recut / diff-remap) carries the link forward
untouched. Only name endpoints can drift; a hash endpoint has no drift
state (see **broken link**). Sub-concept of **drift**.

**Endpoint** **[T][S]** — one side of a link: a name (`@ν`) when the
segment is named — so the link survives content edits — otherwise a
segment hash.

**Endpoint resolution** **[T][S]** — a name endpoint resolves iff a
segment with that name still exists (following it to whatever hash the
segment now carries); a hash endpoint resolves iff that content still
appears in the covering.

**Disconnect** **[T][R][S]** — delete one stored connection ref after
successfully reading it. It is the lifecycle inverse of `connect`, not a
relation-algebra operation: it leaves both coverings, every segment object,
and derived HTML reports untouched.

**Format version** **[R][S]** — the string in `.covmap/version`
(currently `2`) recording the on-disk format. covmap refuses to operate
on a repository whose version disagrees.

**fsck** **[R][S]** — integrity verification: checks covering partitions,
live source against recorded hashes, every present cache object, and
connection endpoints. A missing reconstructible live object is valid, but
unavailable covered source is an integrity failure even when its cached object
is valid.
`--offline` leaves external repositories unchecked; `--hydrate` fills missing
local objects during that pass; `--broken-links` lists just broken links.

**Granularity** **[T][S]** — `g(C) = |C| / N_C ∈ (0, 1]`, where `N_C` is
the total covered line-spans. Near 0 for a freshly-covered repository, 1
for a one-line-per-segment covering.

**HEAD** **[T][R][S]** — the default covering name, recorded in
`.covmap/HEAD` (`main` after `init`). The literal `HEAD` and any bare
address resolve against it.

**HTML report / Compare view** **[R][S]** — the static browser produced
by `covmap html`, with List, Source, Target, and Compare views. Compare
shows the two coverings of a connection side by side for navigating
segment ↔ segment.

**Hydration** **[T][R][S]** — atomically recreating a missing live
**object cache** entry from its checked-out source range after verifying that
the range reproduces the covering ref's recorded hash. Realized by `hydrate`
and `fsck --offline --hydrate`; it never changes refs or accepts drifted bytes.

**Ignore (gitignore-lite)** **[R][S]** — patterns in `.covmap/ignore`
(one per line, `#` comments) that keep build output, vendored, or
generated files out of coverings. Dot-directories are always skipped.

**Iterator** **[T][R][S]** — a covering frozen into an ordered, stateful
*worklist*: each segment becomes a **task** an external consumer (an agent,
a human, a script) pulls one at a time, works against, and marks resolved.
A snapshot layered *over* a covering — it records a per-task state and a
**cursor** but never mutates segments, coverings, or connections; because
iterator creation ensures each task object is present and those objects are
retained append-only, each frozen task's content stays retrievable even after
the covering is edited or deleted. Stored at
`.covmap/iterators/<name>`, a purely additive directory (no format-version
bump). Driven by `covmap iter <name> <verb>`. Has several sub-concepts
(below).

**Task** **[T][R][S]** — one entry of an **iterator**: a segment frozen at
`new` time (its hash, location, label, and name) together with a **task
state** and an optional note. Its content is anchored by the frozen hash;
the line numbers are a hint to the current position. Sub-concept of
**iterator**.

**Task state** **[T][R][S]** — the status of a **task**: `pending` (not yet
resolved), `done` (worked), or `skipped` (deliberately passed over). There
is no stored `active` state — a task is *active* exactly when the **cursor**
points at a still-`pending` task — and no `stale` state, since freezing plus
the retained task object pins its content. Sub-concept of
**iterator**.

**Cursor** **[T][R][S]** — the one task an **iterator**'s consumer is
currently on (the theory's `γ`), unset before the first `next`. A task is
active when the cursor points at a `pending` task. `next` moves it to the
first pending task; `choose` moves it deliberately (a jump); `current`
reprints it (the resume affordance after a crash or context loss).
Sub-concept of **iterator**.

**Complete enumeration** **[T][R][S]** — the invariant an **iterator**
guarantees: every task is eventually resolved (`done` or `skipped`) and
none is stepped over unresolved. Enforced by two rules — `next` advances to
the first `pending` task in the frozen order (globally, not just after the
cursor), and `next` is refused while the current task is still `pending` —
so `finished` is true exactly when no `pending` task remains. It is to a
worklist what the **covering invariants** (C1–C3) are to a covering.
Sub-concept of **iterator**.

**Label** **[T][R][S]** — a per-segment status string, covering-local
metadata that is *not* part of the segment hash (like the segment's name
and message). `cover` assigns `unknown`; typical relabels are `ignored`,
`reviewed`, and `mapped`. Two segments with equal content share one object
regardless of label, and re-labelling never changes a hash.

**Label lens** **[T][R][S]** — an inspection query (`covmap show --label
<l>`) returning every segment of a covering carrying label `<l>` together
with the segments each maps to. Formally the image `κ(S)` of the
label-selected subset `S = C ↾ l` under a connection; the way covmap turns a
cross-cutting concern spread across files into one call. See **Region** for
the file/range-selected analogue.

**Line map** **[T][S]** — a monotone partial injection from old to new
line indices (line `i` maps to `j` only if the lines are equal),
computed by trimming the common prefix/suffix and running LCS on the
residual middle. The position-carry-forward primitive behind **diff-remap**.

**Link** **[T][R][S]** — a single `(source, target)` association inside a
connection, optionally annotated with a note; added by `link`, removed by
`unlink`. A connection is a set of links (the theory: the elements of the
relation `κ ⊆ C_A × C_B`).

**Local state** **[T][R]** — covmap operational state excluded from the
default project artifact: the reconstructible object cache,
checkout-specific peer bindings, iterator work state and retained objects,
generated reports, and temporary files. It may differ across checkouts without
changing the meaning of their coverings or mappings.

**Migrate** **[R][S]** — the one-time `covmap migrate` upgrade from
on-disk format 1 to format 2. It re-keys stored segments by raw content,
rewrites affected covering hashes, connection endpoints, and frozen iterator
tasks, then records format 2. Running it on an already-format-2 repository is
a no-op.

**Broken link** **[T][R][S]** — a link whose source
or target endpoint no longer resolves in its covering: the *terminal*
state, healable only by relinking or dropping (`covmap prune` drops
broken links; `fsck` reports them). Contrast **link drift**, its
repairable precursor. A name endpoint breaks only when the named segment
is deleted or its covering-local name is cleared or changed — a content edit
merely drifts it — whereas a hash endpoint, whose identity *is* the content,
breaks on any content edit, having no drift state to pass through first.
Sub-concept of **link**.

**Resolved link** **[R][S]** — a link presented with its far endpoint
expanded from a bare endpoint (hash or `@name`) to its covering, label, and
location — the form inspection output uses so a reader sees *what* a segment
maps to without joining separate listings by hand. Emitted by
`show`/`ls-links --resolve` and computed by the shared `resolve` module that
also feeds the **HTML report / Compare view**. Sub-concept of **link**.

**Segment message** **[R][S]** — an optional, free-form, possibly
multi-line text recording *why a segment was cut, what it represents, or
why it is (not) mapped* (`req22`). Set with `covmap annotate … -m`, shown by
`show`/`ls -l`/`html`. Like a **segment name** it is covering-local metadata
and **not** part of the **segment hash**, so editing it never touches the
object store or causes **drift**, and it travels with the segment across
**split** (first fragment keeps it) / **merge** (surviving side) /
**recut** / **diff-remap**. Distinct from the two **note** senses: a segment
message is about *the segment itself*, a **link**'s note is about a *mapping*,
and an iterator **task**'s note is about *one sweep step*.

**Segment name** **[T][R][S]** — an optional, stable, human-meaningful
string attached to a segment (e.g. `intro`, `req-1`), unique within its
covering, addressed as `@name`. It is *not* the segment hash and *not*
part of it, so naming, relabelling, or recutting leaves the object store
untouched — which is exactly what lets a connection reference a segment
by name and survive edits to its bytes. Assigned deliberately, never
auto-generated. A segment that carries one is a **named segment**; one
without is **anonymous**.

**Object cache** *(also: object store)* **[T][R][S]** — the
content-addressed cache at `.covmap/objects/<hh>/<rest>`, mapping each
**segment hash** to raw content. Labels, names, and messages live in covering
refs and are not stored in the object. Segments are shared across coverings
automatically by that address. A missing live object can be hydrated from
matching source; a present object must hash to its path. Frozen iterator
objects must be retained because their covering/source coordinates may no
longer be available.

**Partition** **[T][R]** — the decomposition a covering imposes on each
covered file: disjoint, complete, contiguous, file-confined segments.
See **covering invariants**.

**Partition entropy** **[T][S]** — `H(C) = −Σ p·log₂ p` over each
segment's share of the covering's total lines; 0 for a single segment,
`log₂ N_C` for one-segment-per-line, maximized by equal-sized segments. A
one-number gauge of how evenly a covering is cut, and the only measure
distinct from cardinality and granularity. Defined in the theory and
still computed by `stats()`, but **not surfaced in any command output** —
it is not actionable on its own, so it was dropped from `status` (kept in
code, `allow(dead_code)`, for a possible future evenness heuristic).

**Peer alias / peer binding** **[T][R]** — a peer alias is the stable,
project-local name a durable external connection uses instead of an absolute
checkout path. A peer binding is ignored **local state** that resolves that
alias to a repository in one checkout. `.` is the reserved self alias.

**Plumbing / Porcelain** **[T][R][S]** — covmap has a small set of *core
primitives*: scriptable, line-oriented commands (`hash-segment`,
`cat-segment`, `ls-segments`, `ls-links`, `rev-parse`, `fsck`) that speak
hashes on input and output. Every user-facing convenience command
(`cover`, `status`, `split`, `link`, …) is *intended* to be expressible
as a shell pipeline over them — the composability being the point, with
hashes as the glue. This is git's name for the same split: the primitives
are *plumbing*, the convenience commands *porcelain*. (An aspiration, not
a fully-enforced invariant: covmap's porcelain is compiled Rust, and the
plumbing set is thin.)

**Region** **[R][S]** — an inspection selector naming a slice of a covering:
a covered file, or a line range within it (`<C>:<file>[:a..b]`). Not stored —
just a filter over the covering's entries. `covmap show <region>` presents
each segment in the slice with the segments it maps to (the image `κ(S)` for
`S` = the region's segments). The file/range analogue of the **label lens**.

**Position** **[T][S]** — a `(line, optional column)` point in a file. An
absent column (`⊥` / `None`) means the first character of the line for a
start, and the last character (through the trailing newline) for an end;
`(i,1)` and `(i,⊥)` name the same start boundary.

**Project artifact** **[T][R]** — the portable covmap state intended for
publication beside project source: format version, default covering,
covering-input policy, covering refs, connection refs, and the default
publication policy. Together with matching checked-out source, it is sufficient
to verify and reconstruct clean live segments.

**Publication boundary** **[T][R]** — the distinction between a
**project artifact**, which is tracked by default, and **local state**, which
is ignored by default. The ignore policy is repository hygiene rather than an
enforcement fence; publication remains an explicit external action.

**Repair** **[T][R][S]** — bringing a covering's segments back in line
with edited files (drift repair): a *layered fallback chain*, each layer
narrowing what it hands the next — a **segment name** keeps a link alive
→ **recut** / **diff-remap** deterministically re-fit boundaries that
survived in place → **fingerprinting** *proposes* a relocation for what
remains. Repair *restores*; a **broken link** whose endpoint is gone
cannot be repaired, only dropped — that is the separate `covmap prune`
command, deliberately named apart from repair.

The drift-repair layers and their supporting concepts follow (each a
sub-concept of **repair**).

**Recut** **[T][R][S]** — refresh a segment's hash from current disk
bytes, keeping its `(file, a, b, label)` and name. Fixes segment drift;
errors on boundary drift (use `recut --remap` or split/merge). Sub-concept
of **repair**.

**Diff-remap** **[T][R][S]** — `recut --remap`. Reconstructs each covered
file from the covering, diffs it against disk, and carries every
line-aligned segment's boundaries through the unchanged regions
(re-extracting and re-hashing, preserving names and labels). Per file it
is all-or-nothing. Sub-concept of **repair**.

**Fingerprinting** **[T][R][S]** — the last layer. For a segment
diff-remap could not resolve, it ranks candidate windows in the new file
by **similarity score** and prints them (a *fingerprint proposal*). It
never mutates state — see **propose-only**. Sub-concept of **repair**.

**Unresolved segment** **[T][S]** — a segment diff-remap cannot re-fit
(its boundaries fell in changed regions, or the segment moved). Reported
per file and handed to fingerprinting for relocation. `status`/`recut
--remap` count them as *unresolved*. Sub-concept of **repair**.

**Similarity score** **[T][S]** — `sim(τ, w) ∈ [0,1]` between a lost
segment's recorded content `τ` and a candidate window `w`: `1.00`
verbatim, `0.98` equal after normalization (lowercase + whitespace
collapse), else the Jaccard overlap of their word-trigram shingle sets.
Fingerprinting surfaces a candidate only above a threshold `θ`
(default 0.4). Sub-concept of **repair**.

**Propose-only** **[T][R][S]** — the discipline that fingerprinting ranks
and prints relocation candidates but never edits a covering or connection.
A score is a likelihood, not a proof, so the decision to relink is always
left to the user or agent. Sub-concept of **repair**.

**reqmap / self-map** **[R][S]** — named connections in this repo's own
`.covmap/`. `reqmap` links each requirement to the theory block that
justifies it; a self-map connects a covering to another covering of the
*same* repository (e.g. duplication audits, docs-to-code links). The ref
still stores the target repository's canonical absolute path; `status` and
`show` render the relationship as `<source> -> <target> (self-map)`.

**Segment** **[T][R][S]** — a contiguous, labelled range of characters
within a single file: `(file, a, a_col, b, b_col, label)`. The atom of a
covering, identified by its segment hash. Has one sub-concept (below).

**Character-level segment** **[T][S]** — a segment refined below line
granularity, delimited by 1-indexed inclusive character columns
(`file:a:col..b:col`). Columns are optional; a boundary records its column
only when set, so line-only coverings are unaffected. Diff-remap does not
handle char-level segments and falls back to split/merge. Sub-concept of
**segment**.

**Segment hash** *(also: object identity, content hash)* **[T][R][S]** —
the first 64 bits (16 hex chars) of `SHA-256(content)`. It *is* a
segment's identity, so identical content shares one stored object and any
content edit changes it. The label is *not* an input to the hash (it is
ref-local metadata; see **Label**), so re-labelling leaves the hash
unchanged and identical content is one object regardless of label.

**Sharing** **[T][R]** — automatic reuse of a segment across coverings:
two coverings share a segment exactly when its **segment hash** coincides
in both. No duplication in the object store.

**Named segment** **[T][S]** — a segment that carries a **segment name**;
a segment without one is **anonymous**. Being named gives it a second,
edit-stable identity (its name) alongside its hash — see **two-level
identity** — so a connection can reference it by `@name` and survive
content edits.

**Split / Merge** **[T][S]** — the two manual refinement operations.
`split` atomically inserts one or more interior cuts into a segment or covered
file; `merge` removes one cut by fusing two adjacent segments. Every fragment
created by split is `unknown`; the first keeps the original name and message,
while later fragments are anonymous and un-messaged. Merge keeps the surviving
(earlier, if both) name.

**Successor (of an end position)** **[T][S]** — the character position
immediately after a segment's end: the next line's start when the end
ran through a newline, otherwise the next column. Where an adjacent
segment must begin.

**Two-level identity** **[T][R]** — a segment has both a *hash identity*
(its segment hash — immutable, shared) and, when named, a *name identity*
(its name — stable across edits, covering-local). A reference may use
either axis.

**Whole-file segment** **[T][S]** — the single `(f, 1, |f|, unknown)`
segment that **cover** creates for a file before any refinement. Also
useful on its own: a file one is content to track at whole-file
granularity needs no splitting.
