# covmap — Glossary

Terms used across covmap's three descriptions of itself: the Rust
implementation (`src/`), the requirements (`REQUIREMENTS.md`), and the
formal model (`theory/main.tex`). Definitions are kept short and
cross-referenced; where a term is defined precisely in the theory, the
wording here is the informal gloss.

Each entry is tagged with where the term originates:
**[T]** theory · **[R]** requirements · **[S]** source. Entries are
alphabetical, except that a few umbrella concepts (**Drift**, **Repair**)
are immediately followed by their sub-concepts, book-index style, rather
than scattering them across the alphabet. Sub-concepts sit at the same
level as every other entry; the convention is simply that they come right
after their parent. A closing section, *Terminology observations*,
records naming choices made in this revision, concepts whose value is
worth revisiting, and places where the three sources have drifted apart.

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

**Anonymous segment** **[T][S]** — a segment with no name-tag (segment
name). Referable only by its segment hash, so any content edit
invalidates references to it. Contrast **slot** (named segment).

**Cardinality** **[T][S]** — `|C|`, the number of segments in a covering.
A complexity measure reported by `status`.

**Connection** **[T][R][S]** — a many-to-many relation between two
coverings, stored as a set of links with optional per-link notes. This
is the "map" covmap produces. Stored separately from coverings, in the
source repo's `.covmap/connections/<name>`. The two coverings may be in
different repositories or the same one (even the same covering).

**Connection algebra** **[T][R]** — *(defined, not yet implemented)* the
derived operations the model gives connections: *inverse* (`κ⁻¹`),
*composition* (`λ ∘ κ`), and *union*. Specified in the theory and
`req11`, but no CLI command exercises them today. A candidate for a
future release once a concrete need appears (e.g. composing a docs→code
map with a code→test map to derive docs→test).

**Cover** **[T][R][S]** — the `covmap cover` operation that seeds a
covering with one **whole-file segment** (labelled `unknown`) per
readable UTF-8 file. Additive and idempotent: re-running adds only new
files and never disturbs existing refinements. covmap ships no automatic
segmentation rule, so this is the only non-manual step. Has one
sub-concept (below).

**Initial cover** **[T][R][S]** — the particular covering produced by the
first `cover` on a repository: one whole-file `unknown` segment per file,
before any **split**/**merge** refinement. Its **granularity** is near 0.
Sub-concept of **cover**. (Earlier drafts and older theory/requirements
text called this the *bootstrap*; that word is retired.)

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

**Drift** **[T][R][S]** — divergence between a covering's recorded
segments and the current bytes of an edited file: the *repairable* state
that precedes **broken**. Detected without any rule by reconstructing
each file from the covering and comparing it to disk. Has three
sub-concepts (below): the first two distinguished by whether line
boundaries still fit, the third lifting drift from a segment to a link.

**Segment drift** *(also: file-level drift)* **[T][R][S]** — bytes inside
a segment changed but its line boundaries still fit the file. Repaired by
**recut**. Sub-concept of **drift**.

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

**Format version** **[R][S]** — the string in `.covmap/version`
(currently `1`) recording the on-disk format. covmap refuses to operate
on a repository whose version disagrees.

**fsck** **[R][S]** — integrity verification: checks that every covering
is a valid partition of its covered set and that every connection
endpoint resolves. `--broken-pairs` lists just the broken links.

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

**Ignore (gitignore-lite)** **[R][S]** — patterns in `.covmap/ignore`
(one per line, `#` comments) that keep build output, vendored, or
generated files out of coverings. Dot-directories are always skipped.

**Label** **[T][R][S]** — a per-segment status string, part of the
segment hash. `cover` assigns `unknown`; typical relabels are `ignored`,
`reviewed`, and `mapped`. Two segments with equal content but different
labels are distinct.

**Line map** **[T][S]** — a monotone partial injection from old to new
line indices (line `i` maps to `j` only if the lines are equal),
computed by trimming the common prefix/suffix and running LCS on the
residual middle. The position-carry-forward primitive behind **diff-remap**.

**Link** *(also: pair)* **[T][R][S]** — a single `(source, target)`
association inside a connection, optionally annotated with a note; added
by `link`, removed by `unlink`. The theory and code call it a *pair*
(`Pair`); this glossary and the CLI prefer *link*, which reads naturally
in the plural — a connection is a set of links. Has one sub-concept
(below).

**Broken link** *(also: broken pair)* **[T][R][S]** — a link whose source
or target endpoint no longer resolves in its covering: the *terminal*
state, healable only by relinking or dropping (`covmap repair` drops
broken links; `fsck` reports them). Contrast **link drift**, its
repairable precursor. A name endpoint breaks only when the named segment
is *deleted* — a content edit merely drifts it — whereas a hash endpoint,
whose identity *is* the content, breaks on any content edit, having no
drift state to pass through first. Sub-concept of **link**.

**Name-tag** *(also: segment name, slot name)* **[T][R][S]** — an
optional, stable, human-meaningful string attached to a segment (e.g.
`intro`, `req-1`), unique within its covering. It is *not* the segment
hash and *not* part of it, so naming, relabelling, or recutting leaves
the object store untouched — which is exactly what lets a connection
reference a segment by name and survive edits to its bytes. Assigned
deliberately, never auto-generated. The "tag" is just this name string
(nothing to do with the hash); since every segment has a hash but only
some have a name, *segment name* is an accurate plainer synonym.

**Object store** **[T][R][S]** — the hash-addressed store at
`.covmap/objects/<hh>/<rest>`, mapping each **segment hash** to
`label\0content`. Segments are shared across coverings automatically by
that address.

**Partition** **[T][R]** — the decomposition a covering imposes on each
covered file: disjoint, complete, contiguous, file-confined segments.
See **covering invariants**.

**Partition entropy** **[T][S]** — `H(C) = −Σ p·log₂ p` over each
segment's share of the covering's total lines; 0 for a single block,
`log₂ N_C` for one-block-per-line, maximized by equal-sized blocks.
Computed by `stats()` and printed by `status` as a one-number gauge of
how evenly a covering is cut. Purely diagnostic today — nothing consumes
it (see observations).

**Plumbing / Porcelain** **[T][R][S]** — a split borrowed from git. The
underlying idea is that covmap has a small set of *core primitives* — the
scriptable, line-oriented *plumbing* (`hash-segment`, `cat-segment`,
`ls-segments`, `ls-pairs`, `rev-parse`, `fsck`) — from which every
convenience command, the user-facing *porcelain* (`cover`, `status`,
`split`, `link`, …), can be composed as a shell pipeline.

**Position** **[T][S]** — a `(line, optional column)` point in a file. An
absent column (`⊥` / `None`) means the first character of the line for a
start, and the last character (through the trailing newline) for an end;
`(i,1)` and `(i,⊥)` name the same start boundary.

**Repair** **[T][R][S]** — restoring correspondence between a covering
(or connection) and edited files. Two distinct senses share the word:

1. **Drift repair** — bringing a covering's segments back in line with
   edited files, as a *layered fallback chain*, each layer narrowing what
   it hands the next: a **name-tag** keeps a link alive → **recut** /
   **diff-remap** deterministically re-fit boundaries that survived in
   place → **fingerprinting** *proposes* a relocation for what remains.
2. **Connection repair** — the `covmap repair K` command, which drops the
   connection's broken links (endpoints that no longer resolve).

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

**Repair residue** *(also: unresolved slot)* **[T][S]** — the segments
diff-remap cannot re-fit (boundaries fell in changed regions, or the
segment moved). Reported per file and handed to fingerprinting. The code
calls these *unresolved*; the theory calls them *residue*. Sub-concept of
**repair**.

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
*same* repository (e.g. duplication audits, docs-to-code links).

**Segment** **[T][R][S]** — a contiguous, labelled range of characters
within a single file: `(file, a, a_col, b, b_col, label)`. The atom of a
covering, identified by its segment hash. Has one sub-concept (below).

**Character-level segment** **[T][S]** — a segment refined below line
granularity, delimited by 1-indexed inclusive character columns
(`file:a:col..b:col`). Columns are optional; a boundary records its column
only when set, so line-only coverings are unaffected. Diff-remap does not
handle char-level segments and falls back to split/merge. Sub-concept of
**segment**.

**Segment hash** *(also: object identity, truncated hash)* **[T][R][S]** —
the first 64 bits (16 hex chars) of `SHA-256(label ‖ NUL ‖ content)`. It
*is* a segment's identity, so identical labelled content shares one
stored object and any edit changes it. It hashes the label together with
the content, not the content alone — which is why *segment hash* is the
right name for it (covmap no longer calls this a *content hash*).

**Sharing** **[T][R]** — automatic reuse of a segment across coverings:
two coverings share a segment exactly when its **segment hash** coincides
in both. No duplication in the object store.

**Slot (named segment)** **[T][S]** — a segment that carries a
**name-tag**. The theory introduces *slot* to contrast two identities: a
segment's *object identity* (its hash — immutable, shared) and, when
named, its *slot identity* (its name — stable across edits,
covering-local). Operationally a slot is nothing more than a segment
with a name set, so *named segment* says the same thing in plainer
words. See **two-level identity** (and the observation on whether *slot*
earns a separate term).

**Split / Merge** **[T][S]** — the two manual refinement operations.
`split` cuts one segment at a position into a top and a bottom; `merge`
fuses two adjacent segments. Names follow rules: split keeps the name on
the top segment; merge keeps the surviving (earlier, if both) name.

**Successor (of an end position)** **[T][S]** — the character position
immediately after a segment's end: the next line's start when the end
ran through a newline, otherwise the next column. Where an adjacent
segment must begin.

**Two-level identity** **[T][R]** — a segment has both an *object
identity* (its hash — immutable, shared) and, when named, a *slot
identity* (its name — stable across edits, covering-local). A reference
may use either axis.

**Whole-file segment** **[T][S]** — the single `(f, 1, |f|, unknown)`
segment that **cover** creates for a file before any refinement. Also
useful on its own: a file one is content to track at whole-file
granularity needs no splitting.

---

## Terminology observations

Iterating on the glossary surfaced naming choices worth confirming,
concepts whose value is worth revisiting, and places where the three
sources have drifted apart. These are notes for discussion, not defects.

### Naming choices applied in this revision (please confirm)

- **Segment hash — *content hash* retired.** The hash covers
  `label ‖ NUL ‖ content`, not content alone, so *content hash* was
  actively misleading. *Segment hash* is now the only name; T/R/S should
  drop *content hash* (and the "content-addressed" store is reworded as
  hash-addressed by the **segment hash**).
- **Link, not pair.** Entry titled **Link**; the theory/code term *pair*
  is the alias. Matches `link`/`unlink` and reads well in the plural.
- **Segment drift, not file-level drift.** *File-level drift* is kept as
  an alias; *segment drift* names the thing that actually drifted (a
  segment) and parallels **link drift**.
- **Drift is the repairable precursor to broken; both now share a
  spectrum.** A link goes *in-sync → drifted → broken* (for name
  endpoints), or straight *in-sync → broken* (for hash endpoints, which
  have no drift state). Framing *drift* as "repairable" and *broken* as
  "terminal" unifies the two words instead of leaving them as unrelated
  failure modes. New sub-concept **link drift** sits under **Drift**;
  **broken link** now points to it as its precursor.
- **Drift and Repair group their sub-concepts** book-index style (Segment
  / Boundary / Link drift under **Drift**; Recut, Diff-remap,
  Fingerprinting, Repair residue, Similarity score, Propose-only under
  **Repair**) instead of scattering them alphabetically.

### Concepts whose keep-or-cut is worth a decision

- **Slot / name-tag jargon.** A slot is just a *named segment*, and a
  name-tag is just a *segment name*; the hash is a separate thing every
  segment already has. We could drop *slot* and *name-tag* entirely in
  favour of *named segment* and *segment name*, keeping the two-axis
  contrast only where it matters (perhaps as *hash identity* vs. *name
  identity*). One fewer noun to learn; the theory's *object identity /
  slot identity* pairing is the main thing that would need rewording.
- **Connection algebra.** Defined in T and `req11`, not implemented, no
  workflow needs it yet. Elegant and plausibly useful later (composing
  maps). Keep as a documented future direction, or drop from the
  requirements until a use lands?
- **Partition entropy.** Computed and printed, but nothing consumes it
  and it is hard to act on. Is a single evenness number worth a line in
  `status`, or do cardinality + granularity already say enough?
- **Plumbing / porcelain.** The valuable claim is "everything composes
  from a few primitives." If that is the point, we could say exactly that
  and drop the git metaphor — or keep it, since git users recognize it
  instantly.

### Still-unreconciled vocabulary across T / R / S

- **block ≈ segment.** Theory prose says *block* (and "Block lookup");
  code and interface always say *segment*. Standardize on *segment*,
  reserve *block* for informal prose.
- **residue (T) = unresolved (S).** Folded into **Repair residue**; the
  sources themselves still use two words.
- **auto-heal rate (T) vs. healed / refreshed counts (S output).** Same
  quantity, described as a ratio in theory and as counts in CLI output.

### Model gaps (theory vs. code)

- **Link drift is glossary-introduced, not yet in T/R/S.** The sources
  define drift only for *segments* and breakage only for *links*; the
  in-sync → drifted → broken spectrum, and "link drift" as the middle
  state, is this glossary's synthesis of `def:broken` +
  `endpoint_resolves` + the name/hash distinction. Worth promoting into
  the theory (and `req14`) if we agree the framing holds.
- **`repair` is overloaded.** It names both the layered *drift* repair
  (recut → diff-remap → fingerprint) and the `covmap repair K` command
  that drops *broken links*. Different operations, one word — worth
  disambiguating in the docs.
- **Positional address `<file>:<line>[:<col>]`** is a first-class address
  in the code (`resolve_position`) but absent from the theory's
  Addressing table and `req16`.
- **Self-connection** (a covering linked to another, or the same,
  covering of one repo) is implemented and used (`reqmap`, self-map), but
  the theory frames connections as strictly between two repos `A` and
  `B`; it should state that `A = B` is allowed.
- **Stale drift sentence** in theory §3.3 ("recut for file-level drift,
  merge + split for boundary drift") predates the layered chain in
  §3.4–3.5 and `req13`, and reads as superseded.
- **`N_C` contradiction.** Theory defines `ℓ(s) = b − a + 1` (so
  char-level cuts *grow* `N_C`, matching `stats()`) but the next sentence
  says char-level cuts leave `N_C` unchanged. Reconcile the two, and
  restate granularity's `(0, 1]` range accordingly.
