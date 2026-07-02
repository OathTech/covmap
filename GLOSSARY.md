# covmap — Glossary

Terms used across covmap's three descriptions of itself: the Rust
implementation (`src/`), the requirements (`REQUIREMENTS.md`), and the
formal model (`theory/main.tex`). Definitions are kept short and
cross-referenced; where a term is defined precisely in the theory, the
wording here is the informal gloss.

Each entry is tagged with where the term originates:
**[T]** theory · **[R]** requirements · **[S]** source. A closing
section, *Terminology observations*, records aliases (one concept under
several names) and places where the three sources could be reconciled.

---

**Address** *(also: address form)* **[T][R][S]** — a stable, textual
reference to an object. Five forms are accepted and mixed freely:
`<hash>` (a segment by full or unique hex prefix), `<C>:<h>` (segment
`h` in covering `C`), `<C>:@<name>` (a slot by its name-tag),
`<file>:<line>[:<col>]` (the segment containing a position), and a bare
connection name `K`. A bare `<h>` is short for `HEAD:<h>`.

**Adjacency** **[T][S]** — two segments in the same file are *adjacent*
when the successor of the first's end position equals the second's
start. At line granularity this reduces to `b₁ + 1 = a₂`. Only adjacent
segments can be merged.

**Anonymous segment** **[T][S]** — a segment carrying no name-tag. It can
be referenced only by its content hash, so any content edit invalidates
references to it. Contrast **slot**.

**Bootstrap** **[T][R][S]** — the `covmap cover` operation that seeds a
covering with one whole-file `unknown` segment per readable UTF-8 file.
Additive and idempotent: re-running adds only new files and never
disturbs existing refinements. covmap ships no automatic segmentation
rule; bootstrap is the only non-manual step.

**Boundary drift** **[T][R][S]** — drift in which an edit added or removed
lines so a segment's recorded line range no longer fits the file
(`b > |f|` on disk). Repaired by diff-remap, or manually by merge + split.
Contrast **file-level drift**.

**Broken pair** **[T][R][S]** — a connection pair whose source or target
endpoint no longer resolves in its covering. A hash endpoint breaks on
any content edit; a name endpoint breaks only when the named slot is
deleted.

**Cardinality** **[T][S]** — `|C|`, the number of segments in a covering.
A complexity measure reported by `status`.

**Character-level segment / column** **[T][S]** — a segment refined below
line granularity, delimited by 1-indexed inclusive character columns
(`file:a:col..b:col`). Columns are optional; a boundary records its
column only when set, so line-only coverings are unaffected. Diff-remap
does not handle char-level slots and falls back to merge/split.

**Connection** **[T][R][S]** — a many-to-many relation between two
coverings, stored as a set of pairs with optional per-pair notes. This
is the "map" covmap produces. Stored separately from coverings, in the
source repo's `.covmap/connections/<name>`.

**Connection algebra** **[T][R]** — the derived operations on connections:
*inverse* (`κ⁻¹`), *composition* (`λ ∘ κ`), and *union* of two
connections with the same endpoints.

**Content hash** *(also: object identity, truncated hash, segment
identity)* **[T][R][S]** — the first 64 bits (16 hex chars) of
`SHA-256(label ‖ NUL ‖ content)`. It *is* a segment's identity, so
identical labelled content shares one stored object; any edit changes
it.

**Coverage (source / target)** **[T][S]** — the fraction of a covering's
segments that appear in at least one pair of a connection. A connection
is *source-total* when its source coverage is 1.

**Covered set** *(also: covered files)* **[T][R][S]** — the set `F` of
files a covering declares responsibility for partitioning. A covering
need not span the whole repository. On disk, `# covered=` lines.

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
`p₀ ≺ p₁ ≺ … ≺ p_k` delimiting a file's blocks. `split` introduces a
new cut; `merge` removes one. Diff-remap carries interior cuts through an
edit via the line map.

**Diff-remap** **[T][R][S]** — the middle repair layer (`recut --remap`).
Reconstructs each covered file from the covering, diffs it against disk,
and carries every line-aligned slot's boundaries through unchanged
regions, re-extracting and re-hashing while preserving names and labels.
Per file it is all-or-nothing.

**Drift** **[T][R][S]** — divergence between a covering's recorded
segments and the current bytes of an edited file. Detected without any
rule by reconstructing each file from the covering and comparing it to
disk. Split into **file-level drift** and **boundary drift**.

**Endpoint** **[T][S]** — one side of a connection pair: a name-tag
(`@ν`) when the segment is a slot (so the link survives content edits),
otherwise a content hash.

**Endpoint resolution** **[T][S]** — a name endpoint resolves iff a slot
with that name still exists (following it to whatever hash the slot now
carries); a hash endpoint resolves iff that content still appears in the
covering.

**File-level drift** **[T][R][S]** — drift where bytes inside a segment
changed but its line boundaries still fit the file. Repaired by `recut`.
Contrast **boundary drift**.

**Fingerprinting** **[T][R][S]** — the last repair layer. For a slot
diff-remap could not resolve, it ranks candidate windows in the new file
by similarity and prints them. Strictly **propose-only**: it never
mutates a covering or connection.

**Format version** **[R][S]** — the string in `.covmap/version`
(currently `1`) recording the on-disk format. covmap refuses to operate
on a repository whose version disagrees.

**fsck** **[R][S]** — integrity verification: checks that every covering
is a valid partition of its covered set and that every connection
endpoint resolves. `--broken-pairs` lists just the broken pairs.

**Granularity** **[T][S]** — `g(C) = |C| / N_C ∈ (0, 1]`, where `N_C` is
the total covered line-spans. Near 0 for a bootstrap covering, 1 for a
one-line-per-segment covering.

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
content hash. Bootstrap assigns `unknown`; typical relabels are
`ignored`, `reviewed`, and `mapped`. Two segments with equal content but
different labels are distinct.

**Line map** **[T][S]** — a monotone partial injection from old to new
line indices (line `i` maps to `j` only if the lines are equal),
computed by trimming the common prefix/suffix and running LCS on the
residual middle. The position-carry-forward primitive behind diff-remap.

**Name-tag** *(also: slot name, tag)* **[T][R][S]** — a stable,
covering-local, human-meaningful identifier assigned to a segment. It is
*not* part of the content hash, so naming, relabelling, or recutting
leaves the object store untouched. Assigned deliberately, never
auto-generated; unique within a covering.

**Object store** **[T][R][S]** — the content-addressed store at
`.covmap/objects/<hh>/<rest>`, mapping each hash to `label\0content`.
Segments are shared across coverings automatically by content address.

**Pair** **[T][S]** — a single linked `(source, target)` tuple in a
connection, optionally annotated with a note. Added by `link`, removed
by `unlink`.

**Partition** **[T][R]** — the decomposition a covering imposes on each
covered file: disjoint, complete, contiguous, file-confined blocks. See
**covering invariants**.

**Partition entropy** **[T][S]** — `H(C) = −Σ p·log₂ p`, over each
segment's fraction of total covered lines. Ranges from 0 (single block)
to `log₂ N_C` (one block per line); maximized at fixed cardinality by
equal-length blocks.

**Plumbing / Porcelain** **[T][R][S]** — the command split. *Porcelain*
is the user-facing surface (`cover`, `status`, `split`, `link`, …);
*plumbing* is the scriptable, line-oriented layer (`hash-segment`,
`cat-segment`, `ls-segments`, `ls-pairs`, `rev-parse`, `fsck`). Every
porcelain command is expressible as a pipeline of plumbing.

**Position** **[T][S]** — a `(line, optional column)` point in a file.
An absent column (`⊥` / `None`) means the first character of the line
for a start, and the last character (through the trailing newline) for
an end; `(i,1)` and `(i,⊥)` name the same start boundary.

**Propose-only** **[T][R][S]** — the discipline that fingerprinting ranks
and prints relocation candidates but never edits state on its own. A
similarity score is a likelihood, not a proof, so adjudication is left
to the user or agent.

**Recut** **[T][R][S]** — refresh a segment's hash from current disk
bytes, keeping its `(file, a, b, label)` and name. Repairs file-level
drift; errors on boundary drift (use `recut --remap` or merge/split).

**reqmap / self-map** **[R][S]** — named connections in this repo's own
`.covmap/`. `reqmap` links each requirement to the theory block that
justifies it; a self-map connects a covering to another covering of the
*same* repository (e.g. duplication audits, docs-to-code links).

**Residue** *(also: unresolved slot)* **[T][S]** — the slots diff-remap
cannot re-fit (boundaries fell in changed regions, or the slot moved).
Reported per file and handed to fingerprinting for relocation.

**Segment** **[T][R][S]** — a contiguous, labelled range of characters
within a single file: `(file, a, a_col, b, b_col, label)`. The atom of a
covering, identified by its content hash.

**Sharing** **[T][R]** — automatic reuse of a segment across coverings:
two coverings share a segment exactly when its content hash coincides in
both. No duplication in the object store.

**Similarity score** **[T][S]** — `sim(τ, w) ∈ [0,1]` between a slot's
recorded content `τ` and a candidate window `w`: `1.00` verbatim,
`0.98` equal after normalization (lowercase + whitespace collapse), else
the Jaccard overlap of their word-trigram shingle sets. Surfaced only
above a threshold `θ`.

**Slot** **[T][S]** — a segment that carries a name-tag. Its *slot
identity* (the name) is stable across content edits and local to one
covering, complementing its content-hash *object identity*.

**Split / Merge** **[T][S]** — the two manual refinement operations.
`split` cuts one segment at a position into a top and a bottom; `merge`
fuses two adjacent segments. Name-tags follow rules: split keeps the
name on the top block; merge keeps the surviving (earlier, if both)
name.

**Successor (of an end position)** **[T][S]** — the character position
immediately after a segment's end: the next line's start when the end
ran through a newline, otherwise the next column. Where an adjacent
segment must begin.

**Two-level identity** **[T][R]** — a segment has both an *object
identity* (its content hash — immutable, shared) and, when named, a
*slot identity* (its name-tag — stable across edits, covering-local). A
reference may use either axis.

**Whole-file segment** **[T][S]** — the single `(f, 1, |f|, unknown)`
segment `cover` creates for each file before any refinement.

---

## Terminology observations

Writing the glossary surfaced a few places where one concept wears
several names, and a few where the three sources have drifted apart.
These are notes for consideration, not defects.

### Aliases — one concept, several names

- **block ≈ segment ≈ slot.** The theory speaks of partitioning a file
  into *blocks* (and "Block lookup"), while both the code and the
  interface only ever say *segment*; a named segment is a *slot*. "Block"
  is theory-only vocabulary for the same partition atom the
  implementation calls a segment. Consider standardizing on *segment*
  (reserving *block* for prose) so readers don't hunt for a distinct
  "block" object.
- **name-tag = slot name = tag.** The theory and CLI help say
  *name-tag*; the code fields and `valid_name` say *name* / *slot name*;
  the `name` command's argument is `<tag>`. One concept, three spellings.
- **object identity = content hash = truncated hash = segment identity =
  64-bit identity.** All name the same value; the glossary folds them
  into one entry.
- **residue = unresolved (slots).** The theory calls the leftover slots
  *residue*; the code (`RemapError::Unresolved`, `status`/remap output)
  calls them *unresolved*.
- **auto-heal rate (theory) vs. "healed" / "remapped" (CLI output).**
  The theory's fraction-of-cuts-in-unchanged-regions is reported
  operationally as counts of healed files and refreshed slots.
- **covered set F (theory) = covered files / `# covered=` (code).**

### Possible refinements

- **Stale drift-repair sentence in the theory.** §3.3 still says the two
  drift flavours map to "recut for file-level drift, merge + split for
  boundary drift." §3.4–3.5 and `req13` then introduce the layered chain
  (name-tag → diff-remap → fingerprinting). The earlier sentence predates
  the layered repair and reads as superseded; it could point forward to
  the three-layer fallback chain.
- **Self-connection is implemented but under-modelled.** The theory's
  abstract and `Connection` definition frame covmap as linking *two*
  repositories `A` and `B`, yet `req2`, the `reqmap`/self-map connections,
  and the code all support connecting a covering to another covering of
  the *same* repository (even the same covering). The theory could state
  explicitly that `A = B` is allowed.
- **The positional address form is missing from theory and requirements.**
  The code implements `<file>:<line>[:<col>]` as a first-class address
  (`resolve_slot` / `resolve_position`), but the theory's Addressing
  table lists only `<hash>`, `C:h`, `C:@ν`, and `K`, and `req16`
  enumerates hash / covering-qualified hash / slot name / connection name.
  Adding the positional form would align all three.
- **`N_C` under character-level cuts is described two ways.** The theory
  defines `ℓ(s) = b − a + 1` with "a character-level segment within one
  line has `ℓ = 1`", which makes `N_C = Σ ℓ(s)` *grow* when a line is cut
  into several char-level segments (the implementation's `stats()` sums
  the same `b − a + 1` and agrees). But the following sentence states
  char-level cuts increase cardinality "without changing `N_C`". The two
  claims conflict; reconciling them (and restating granularity's `(0,1]`
  range accordingly) would make the complexity measures unambiguous.
