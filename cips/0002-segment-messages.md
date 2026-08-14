# covmap segment messages — recording *why* a segment exists

## Idea

Add an optional, free-form **message** to a segment: human-readable text
recording *why this segment was cut, what it represents, or why it is
(not) mapped*. It may span multiple lines (like a `git commit` message).
It is covering-local metadata attached to the segment itself — not a
category (`label`), not an identity (`name`), and not a statement about a
relationship (`link` note) or a work-step (`iter` task note). Set it with
`covmap annotate <C>:<addr> -m "<text>"`.

We call it a *message* — matching the `-m`/`--message` flag, and keeping it
verbally distinct from the *notes* that live on links and iterator tasks.

## Motivation

covmap ships **no segmentation rule**: every boundary is a manual
judgment. That judgment is expensive, non-obvious knowledge, and today it
is lost the instant it is made — the covering records *that* a segment
exists, never *why*. On a large hand-built covering the next agent or
reviewer must re-derive the rationale from scratch.

The existing text channels do not serve this:

- **`label`** is part of the segment hash, so it is for coarse, re-hashable
  categories (`unknown`/`reviewed`/`mapped`), not prose.
- **`name`** is a constrained slug identity (no spaces/`:`), not a sentence.
- **link notes** annotate a *mapping* (`κ`), not the segment; they vanish
  if the segment is never linked.
- **iterator task notes** annotate a *sweep step*; they are transient and
  disappear on `iter rm`.

A message fills the gap: durable, possibly multi-paragraph rationale about
the segment, independent of any connection or iterator. It is the
segmentation counterpart of the durable, self-explaining artifact covmap
already gives mappings.

## Model

A message is a piece of covering-local, **non-hashed** segment metadata,
implemented like `name` (a field on the segment that travels with it). It
is *not* an input to `hash`, so setting, changing, or clearing a message
leaves the shared object store untouched and never triggers drift. It needs
no theory change: the model already admits non-hashed, covering-local
metadata on a segment (that is what `name` is), and whether such a field
holds a name or a message is an implementation detail the theory does not
distinguish.

**Why not hashed.** If the message were part of the segment hash, editing it
would change the segment's identity — breaking hash-linked connections and
forcing a recut/relink for a pure documentation change. That drift-repair
burden is exactly what we want to avoid for an annotation whose whole point
is to be edited freely. Keeping the message out of the hash makes editing it
local and cost-free. (The trade-off — two segments with identical
content+label but different messages still share one stored object — is
correct: the message is *about this covering's choice*, not the content.)

### On-disk format

Because a message can be multi-line, it does **not** go in a tab field on
the one-segment-per-line record. Segment lines are unchanged. Messages are
appended after all segment lines as length-prefixed stanzas:

```
# covered=<file>
<hash>\t<file>\t<a>\t<b>\t<label>[\t<name>]
...
# message <index> <line-count>
<message line 1>
<message line 2>
...
```

- `<index>` is the 0-based position of the segment among the ref's segment
  lines (file order, as written). It is only an intra-file join between a
  segment and its message — not a durable external key — so it is recomputed
  on every write and is robust to duplicate hashes.
- `<line-count>` fixes exactly how many following lines form the message, so
  the body is read verbatim: blank lines, and even lines beginning with `#`,
  are unambiguous.

A segment with no message emits no stanza. Older refs (no `# message`
stanzas) parse unchanged, so this is backward compatible: **no
`FORMAT_VERSION` bump**, as with the char-column and name additions.

In memory the message is `Option<String>` on `CoveringEntry`; the stanza
form is purely how it serializes.

### Propagation across refinement

The message travels with its segment, following the same rules as `name`,
so it survives content edits:

- **`split`** — the top segment keeps the message; the bottom is un-messaged
  (a fresh segment).
- **`merge`** — the merged segment keeps the surviving side's message (the
  earlier side's if both have one; warn on the dropped one, as `merge`
  already does for names).
- **`recut`, `label`, `recut --remap`** — preserve the message.
- **`name`** — orthogonal; setting/clearing a name never touches the message.

## Commands / API

| Command | Purpose |
| ------- | ------- |
| `covmap annotate <C>:<addr> -m <text>` | Set or replace the segment's message. Repeatable: multiple `-m` join as paragraphs separated by a blank line (git convention); a single `-m` with embedded newlines is also accepted. Re-annotating replaces the whole message in place. |
| `covmap annotate <C>:<addr> -` | Clear the message. |
| `covmap annotate <C>:<addr>` | Print the segment's current message (no `-m` = read). |

`<addr>` is any segment address (`@name`, hash prefix, or `file:line[:col]`),
consistent with every other segment-address command. The message is
surfaced, read-only, by existing inspection commands:

- `covmap show <C>:<addr>` — prints the full (multi-line) message under the
  segment.
- `covmap ls -l [<C>]` — appends the message's **first line** as a trailing
  column (truncated for width; empty when unset), so the listing stays
  one-row-per-segment; `show` is the way to read the whole thing.
- `covmap html` — renders the message on the segment (detail panel).

## Invariants

- A message is **not** part of `hash(s)`; it is covering-local metadata.
- Setting, changing, or clearing a message touches neither the object store
  nor any connection; it never causes or repairs drift.
- The message propagates across `split`/`merge`/`recut`/`recut --remap`/
  `label` by the rules above, so it survives edits the way a `name` does.

## Use cases

- **Segmentation rationale.** `covmap annotate main:@bs-appendBit -m
  "Level-2/3/4 core; isolated so its invertibility proof maps 1:1"` — the
  next reader sees *why* the boundary is there.
- **Durable skip reason.** A segment deliberately never mapped: `annotate
  main:@vendored-crc -m "generated table, out of spec scope"` — permanent,
  unlike an iterator `skip -m` that dies with the sweep.
- **Provenance.** `annotate main:@x509-validity -m "RFC 5280 §4.1.2.5"` —
  what the chunk corresponds to, independent of any mapping.
- **Longer rationale.** A multi-paragraph message (repeated `-m`) capturing a
  subtle boundary decision and the alternatives rejected — the kind of thing
  that otherwise lives only in a commit message or a reviewer's head.

## Landing checklist (six-artifact consistency)

Per `DEVELOPER.md`, this is a **new capability**, not done until reconciled
across the artifacts:

- **Theory** (`theory/main.tex`): **no change — consciously out of scope.**
  The model already treats a segment's non-hashed, covering-local metadata
  abstractly; whether such a field carries a name, a label, or a message is
  an implementation concern, not a mathematical one.
- **Requirements** (`REQUIREMENTS.md`): add a new **req22** (segment
  messages / annotation). A message is not identity (that is req3) — it
  unlocks new capability (durable rationale, provenance, review flags), so
  it warrants its own requirement rather than amending req3. Map it via
  `reqmap`.
- **Implementation** (`src/`): a `message: Option<String>` on
  `CoveringEntry`; read/write the `# message` stanzas; propagate through
  `split`/`merge`/`recut`/`remap`; `cmd_annotate` in `cli.rs`; surface in
  `show`/`ls -l`/`html`; in-module tests (multi-line roundtrip, legacy
  parse, `#`/blank-line bodies, propagation, clear).
- **Skill** (`SKILL.md`): document `annotate`, and add a short **"where text
  lives"** table making the five channels crisp — `label` (hashed category),
  `name` (slug identity), **segment message (why this segment)**, link note
  (why this mapping), iterator task note (this sweep's verdict).
- **Tutorial** (`tutorial/`): a step that annotates a segment with its
  rationale and shows it in `ls -l`/`show`, with an assertion; narrate the
  same five-channel distinction in `TUTORIAL.md`.
- **`GLOSSARY.md`**: a **Segment message** entry, and a tightened wording of
  the surrounding vocabulary so the five text channels read as one system —
  cross-reference **Label**, **Segment name**, and the two **note** senses
  (link note, task note) so message-vs-note is unambiguous.

No `FORMAT_VERSION` bump — the `# message` stanzas are purely additive.
