# Content-only segment hash — label as ref-local metadata

## Idea

Today a segment's identity is the truncated SHA-256 of its **label concatenated
with its content**:

```
hash(s) = trunc16( sha256( label ‖ NUL ‖ content ) )
```

This cip removes the label from the hash, so identity is a pure function of the
mapped bytes:

```
hash(s) = trunc16( sha256( content ) )
```

Label then joins **name** and **message** as *ref-local metadata*: recorded in
the covering ref, never in the object, never in the hash. All three per-segment
annotations become uniform — non-hashed, editable without touching the object
store, and travelling with the segment across split/merge/recut/remap.

This reverses **req4** ("labels as part of content identity") and edits the
theory's segment definition. It is an on-disk **format change** with a migration,
hence a `FORMAT_VERSION` bump (the first real one) — kept separate from the
output-only cip 0003.

## Motivation

- **A hash should be consistent only with the file it maps.** Today the hash
  mixes content with a covmap-internal status word (`unknown`, `mapped`, …), so
  two byte-identical spans hash differently if labelled differently, and
  *re-labelling a span with no file edit changes its hash*. Content-only hashing
  makes the content address exactly that — an address of content, nothing else.
- **Uniform annotation model.** covmap already made `name` (req3) and `message`
  (req22) non-hashed, ref-local metadata precisely so they can change without
  disturbing identity or links. `label` is the lone holdout. Folding it in is the
  natural completion of a direction the tool already took twice.
- **Removes a footgun and unblocks the label lens.** Because re-labelling
  currently re-hashes, it silently breaks *hash*-linked endpoints. cip 0003's
  label lens documented this as constraint (a). Content-only hashing removes it:
  a segment can be (re)labelled freely, even when hash-linked, without breaking
  the link. (cip 0003's constraint (b), *label is singular*, is untouched — a
  multi-valued `tag` channel remains a separate future question.)
- **Cleaner drift-repair** (see below): identical content becomes identical
  identity, so exact-match relocation is a statement about content alone.

## Why it helps AI agents

Agents relabel as they work (`unknown` → `mapped` → `reviewed`). Today each such
edit mints a new identity and can break a hash-link the agent didn't realize it
touched — a silent, surprising failure. After this change, labelling is a safe,
metadata-only act with no identity or link consequences, so an agent can annotate
freely as understanding grows. It also makes the mental model an agent must hold
smaller: *object = content; ref = all metadata.*

## Model

### Object store

- **Before:** object body is `label ‖ NUL ‖ content`; keyed by
  `trunc16(sha256(label ‖ NUL ‖ content))`.
- **After:** object body is `content` (no label, no NUL prefix); keyed by
  `trunc16(sha256(content))`.

Consequence: the object is no longer self-describing about its label. Label is
recovered from the **covering ref** (where it already lives, as the 5th field),
not from the object. Addressing a segment by **bare hash** (no covering context)
therefore has no label — which is correct: label is covering-local metadata, and
a bare hash names content across all coverings that share it.

### Covering ref

**Unchanged on disk.** Label is already the 5th field of a covering row
(`hash⇥file⇥a⇥b⇥label[⇥name]`). What changes is only that this field is now the
*sole* home of the label, and the `hash` column is recomputed from content alone.

### Theory (`def:segment`)

The segment tuple keeps its shape `s = (f, a, a_c, b, b_c, λ)` — a segment still
*has* a label λ — but the **hash drops λ**:

```
hash(s) = trunc16( h( content_A(s) ) )
```

and `label(s) = λ` becomes an *annotation* on the segment (like the
already-non-hashed name), not a component of its identity. The "why 64-bit
identity" birthday-bound remark is unaffected (same hash width, same input size
class).

### Identity consequence

Two segments with identical content but different labels, today distinct objects,
become the **same object** with one shared hash. Per-segment **status still
works**: status is the `label` field stored *per covering row*, not derived from
object identity, so a segment can be `unknown` in one covering and `mapped` in
another while sharing content — arguably an improvement over today, where they
are forced to be different objects.

## Migration (`FORMAT_VERSION` 1 → 2)

Every object is keyed by the old label-inclusive hash, and every ref (coverings,
connections, iterators) references those hashes. Migration, per repo:

1. For each covering entry: recompute `new = trunc16(sha256(content))`, write the
   object body as pure `content` under `new`, and rewrite the entry's hash column
   `old → new` (label field retained verbatim).
2. Rewrite every **bare-hash** connection endpoint and iterator cursor/task hash
   `old → new`. **`@name` endpoints are unaffected** (they never referenced a
   hash).
3. Bump `.covmap/version` to `2`.

Old objects may be left in place (the store is append-only and immortal) or
garbage-collected; refs point only at the new hashes.

**Cross-repo hazard (must be documented).** A connection stored in repo A may
reference a **target-repo** endpoint by bare hash; that hash changes when the
*target* repo migrates. So a bare-hash cross-repo link resolves again only once
**both** repos are migrated. `@name` cross-repo links are immune. Recommendation:
migration warns when a connection has bare-hash endpoints into a repo whose
version is still `1`, and `fsck` already reports the resulting unresolved
endpoints. This is the strongest practical argument for naming mapped segments.

Delivery: a `covmap migrate` command (explicit, one-shot) rather than
migrate-on-read, so the re-hash is a deliberate, inspectable step with a clear
`fsck` before/after.

## Commands / API

No new user-facing verbs. Surface changes:

- `covmap migrate` — one-shot re-hash of `.covmap/` from format 1 → 2 (above).
- `covmap show <hash>` addressed by **bare hash** no longer prints a label (label
  is ref-local); `show <C>:<h>` and `show <C>:@name` print it as today, from the
  covering. `ls`/`ls -l` unchanged (label comes from the ref).
- Internally, `segment::read_object` returns content only; callers that need a
  label read it from the `CoveringEntry`, not the object. (`hash-segment` no
  longer takes a label into the hash.)

## Invariants

- **Content-only identity.** `hash(s)` depends only on `content_A(s)`; identical
  content ⇒ identical hash, regardless of label.
- **Label / name / message are all ref-local and non-hashed.** Editing any of the
  three never changes a segment's identity, never touches the object store, and
  never causes or repairs drift.
- **Status is preserved.** Per-covering `label` continues to carry per-segment
  status; a segment may hold different labels in different coverings.
- **Post-migration integrity.** After `covmap migrate` + (for cross-repo links)
  migrating the partner repo, `covmap fsck` is clean: every partition valid,
  every endpoint resolves.

## Impact on drift-repair

Net positive; no mechanical breakage. Both repair layers already key on
**content, not the hash**:

- **Fingerprinting** (`fingerprint.rs::locate`) scores candidate windows by
  similarity over raw **content** shingles; it never reads the label or the hash.
- **`recut --remap`** relocates a lost segment by reading its recorded **content**
  and fingerprinting that against the new file; the label plays no role.

The improvement is conceptual: today a moved-and-relabelled block changes its
hash for *two* reasons (relocation and relabel), muddying "is this the same
segment?"; with content-only hashing, exact-match relocation (`window == target`
→ 1.0) is a clean statement about content, and the planned three-layer repair
(name-tags → diff-remap → fingerprinting) rests on a hash that no longer carries a
non-content variable.

## Use cases

- **Free relabelling during a review sweep:** mark segments `reviewed` /
  `mapped` as you go, even hash-linked ones, with zero risk of breaking a link —
  the identity is the content.
- **Content dedup across coverings:** identical text mapped under different status
  labels now shares one object.
- **Label lens (cip 0003) on hash-linked maps:** safe, because tagging no longer
  re-hashes.

## Landing checklist (six-artifact consistency)

Per `DEVELOPER.md`, this is an **on-disk format change** *and* a **requirements
reversal**, reconciled a→b→c→d→e→f, and it is the trigger case for the format-change
protocol:

- **Theory** (`theory/main.tex`): amend `def:segment` so the hash is
  `trunc16(h(content))` and `label(s)=λ` is an annotation, not part of identity;
  align the surrounding prose (the "labelled content" wording) and §5 (state on
  disk) format description. Re-verify no proposition depended on label-inclusive
  identity.
- **Requirements** (`REQUIREMENTS.md`): **reverse req4** — labels are ref-local
  metadata, *not* part of the segment hash — and re-map it to the amended theory
  block via `reqmap`; confirm **req3** (names non-hashed) now reads as one case of
  a general "annotations are non-hashed" rule; confirm **req17** (versioned state)
  holds across the bump.
- **Implementation** (`src/`): `segment.rs` — `hash_of` drops the label;
  `write_object` body = content; `read_object` returns content only; audit every
  `read_object` caller that used the returned label and source it from the
  `CoveringEntry` instead. `repo.rs` — `FORMAT_VERSION = "2"` and a
  `check_format_version` path that directs a version-`1` repo to `covmap migrate`.
  New `covmap migrate` (re-hash objects, rewrite covering/connection/iterator
  hash references, bump `.covmap/version`). `#[cfg(test)]`: hash-excludes-label;
  relabel-preserves-hash; migration round-trip on a fixture repo incl. a
  bare-hash link and an `@name` link.
- **Skill** (`SKILL.md` + `skill_playground/`): update the "five text channels"
  description (label is no longer the hashed one); re-check any task that assumed
  relabel changes the hash (e.g. name-stability / drift tasks) and adjust its
  `grade.sh`/narrative.
- **Tutorial** (`tutorial/`): adjust the `annotate`/label narration and any
  assertion that observed a hash change on relabel; re-run `run_tutorial.sh`.
- **`GLOSSARY.md`**: update *segment* (identity = content), *label* (ref-local
  status metadata), and the segment-identity cross-reference.
- **`README.md`**: update the object-store/format description and the format
  version.

**`FORMAT_VERSION` 1 → 2 with a migration.** This bump must land *with* `covmap
migrate`; bumping it earlier would make the binary refuse existing `.covmap/`
directories (including this repo's own) before a migration path exists. The
product version moves to **0.3.0** to carry the release.
