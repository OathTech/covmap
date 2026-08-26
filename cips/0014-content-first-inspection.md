# Content-first inspection

Stage: **Landed**

## Relationship to landed CIPs

[CIP 0003](0003-bounded-drill-down.md) made focused `show` output omit content
by default and defined region/label views as bounded relationship views.
[CIP 0001](0001-iterators.md) made iterator `next`/`current` terse unless
`--content` was supplied.

This proposal reverses the defaults for focused and spatially bounded views.
It preserves:

- CIP 0003's resolved relationships in region and label views;
- compact label views for cross-cutting, potentially cross-file selections;
- [CIP 0006](0006-mapping-review-surfaces.md)'s bounded summaries;
- CIP 0001's frozen iterator model and enumeration guarantees;
- all stored formats and plumbing output.

## Idea

When covmap has selected one review unit or an explicit spatial region, show
its recorded content by default. A segment, file, line range, mapping link, or
iterator cursor should present the bytes under review without repeated
`--content`.

Open-ended set and summary views remain compact. Keep `--content` for explicit
numbered output, keep `--content-plain` for raw bytes, and add `--no-content`
as the compact compatibility override.

## Motivation

Before this proposal, content-mode spellings occurred 11 times in
`tutorial/TUTORIAL.md`, 13 times in `SKILL.md`, and 5 times in
`tutorial/run_tutorial.sh`. The repetition was concentrated in focused
inspection and iterator-resume workflows.

Showing a covered file should also reveal its covmap decomposition: ordered
segments, their relationships, and their recorded content. This is bounded by
the file or line range named in the command. Cross-file label selections and
whole-covering or whole-connection summaries remain compact. The command
matrix below is authoritative.

## Command contract

The existing `--content` / `--content-plain` mutual-exclusion error is extended
to a third mode:

```text
--content         numbered recorded content
--content-plain   raw recorded bytes
--no-content      no content block
```

No flag selects the view-specific default. Repeating or combining modes is an
error. `--content` remains accepted when numbered output is already the
default, preserving existing commands and allowing callers to pin the format.

### Content-capable views

All three modes are accepted:

| Surface | Default |
| ------- | ------- |
| `show <C>:<segment>` | numbered |
| `show <bare-hash>` | numbered (unchanged) |
| `show <C>:<file>` | numbered for selected source segments, in file order |
| `show <C>:<file>:a..b` | numbered for intersecting source segments, in file order |
| `show --label <label> [<C>]` | none |
| `show <K> <A> <B>` | numbered for both endpoints |
| `iter <I> next` / `current` | numbered frozen task content |
| `iter <I> peek [--all]` | none |

Explicit `--content` still requests all selected label or peek content.
`peek --no-content` is accepted as a uniform explicit compact mode.

### Summary views

`show <covering>` and `show <connection>` never render content:

- `--content` and `--content-plain` error with guidance to select a segment,
  region, label, or single link;
- `--no-content` is accepted idempotently, allowing callers to request compact
  output defensively across all `show` routes.

Unknown flags already error, while recognized-but-inapplicable positive
content flags are currently silent no-ops. This proposal makes positive modes
either affect the selected route or fail; `--no-content` is the deliberate
idempotent exception.

`--all` is valid only on `show <connection>` and errors on every other route.

### Validation order

Argument errors are selected in this order:

1. unknown, missing-value, repeated, or conflicting flags;
2. route/arity resolution;
3. route-specific applicability;
4. rendering.

Therefore `show <K> --content --all` reports that `--content` is invalid for a
connection summary; valid `--all` does not mask the invalid content mode.
`show <K> --no-content --all` succeeds.

## `show` behavior

### Focused segment

If content cannot be read, render available metadata and links, diagnose the
failure, and return non-zero.

### File and line-range region

Render selected source segments in file order. For each segment, retain its
resolved relationship lines and then print its recorded content. Do not expand
the mapped target segments' content; a reader can inspect one link for that
two-sided view. `--no-content` retains the current relationship-only view.
A line range selects overlapping segments and renders each selected segment's
full recorded content; it does not clip a segment at the query boundaries.

If content cannot be read, do not suppress relation information. Render the
segment's metadata and links, diagnose unavailable content, continue with
later segments, and return non-zero after the complete view.

### Label lens

The label lens remains compact by default because its selection may cross many
files. With explicit content, it follows the same contextual failure and
continue-to-completion behavior as a region.

### Single link

The inspection form is:

```text
covmap show <K> <A> <B>
```

This proposal does not change the mutating `covmap link` command.

Keep the current link note, status, and endpoint metadata, then print
role-qualified content blocks:

```text
--- content source main:@req1 ---
17 | ...
--- content target theory:@covering_partition ---
142 | ...
```

Role and covering distinguish the two sides, including self-links. Names also
identify one concrete occurrence. For an anonymous hash shared by several
covering entries, existing first-occurrence location semantics remain; the
content itself is identical because the hash is content identity.

Resolve a named endpoint directly to its concrete entry instead of resolving
to a hash and then selecting a different equal-content occurrence.

For degraded links:

- a broken endpoint has no content block and is diagnosed as broken;
- an unavailable external target remains `unchecked`, not `broken`;
- local source metadata and content remain visible when the target is
  unavailable;
- unreadable or corrupt content for a resolved local endpoint is diagnosed
  after available link context, and the command returns non-zero.

Default target-content presentation is best effort. It does not turn
single-link inspection into a req20 command that requires target bytes to
succeed; an unavailable peer follows the existing degraded read-only path.

`--no-content` retains the current metadata-only link view.

## Iterator behavior

Whenever `next` or `current` renders content, it uses the task's frozen object
bytes, never the current live range.

The state machine is unchanged. `next` still advances to the first pending
task and is refused while the current task is pending. If a retained object
cannot be read, preserve the existing warning and cursor behavior so the
consumer can retry with `current`.

This proposal does not add links to iterator output. Iterators freeze task
content and state, not connection relations. Live links would change over
time; frozen links require a separate persisted model.

## Use cases and output

Ellipses omit unchanged metadata.

### Covering-qualified segment

Before:

```text
$ covmap show main:@req1
segment 7812142549dd7f3a
  covering  main
  ...
  links     2 outgoing
```

After:

```text
$ covmap show main:@req1
segment 7812142549dd7f3a
  covering  main
  ...
  links     2 outgoing
--- content @req1 ---
17 | ## req1 - Cover a single repository
...
```

### File decomposition

The spatial selector itself requests a bounded walkthrough. Each source
segment keeps its resolved relationships and gains a content block:

```text
$ covmap show main:REQUIREMENTS.md
region main REQUIREMENTS.md · 2 segment(s)
  @req1  mapped  REQUIREMENTS.md:17..20
    -> @covering_partition   theory/main.tex:142..163
--- content @req1 ---
17 | ## req1 - Cover a single repository
...
  @req2  mapped  REQUIREMENTS.md:21..29
    -> @well_formed   theory/main.tex:164..181
--- content @req2 ---
21 | ## req2 - Maintain a well-formed covering
...
```

`show main:REQUIREMENTS.md:18..22` selects both segments because the range
straddles their boundary, and renders each selected segment in full.
`--no-content` suppresses the content blocks without removing the relationship
lines.

### Single link

Before:

```text
$ covmap show reqmap @req1 @covering_partition
link in reqmap
  note
    covering as the partitioned unit
  status  resolved   (both endpoints present in their coverings)
  source  main : mapped REQUIREMENTS.md:17..20
  target  theory : mapped theory/main.tex:142..163
```

After, the same command appends:

```text
--- content source main:@req1 ---
17 | ## req1 - Cover a single repository
...
--- content target theory:@covering_partition ---
142 | ...
```

### Iterator cursor

Before:

```text
$ covmap iter review next
@req1	mapped	REQUIREMENTS.md:17..20
```

After:

```text
$ covmap iter review next
@req1	mapped	REQUIREMENTS.md:17..20
--- content @req1 ---
17 | ## req1 - Cover a single repository
...
```

### Inapplicable summary flag

```text
$ covmap show main --content
covmap: show covering 'main': --content is not valid for a covering summary; select a segment or region
```

Connection summaries use the analogous error and direct the user to a single
link.

## Compatibility and plumbing

This is an intentional porcelain-output change:

- existing explicit content calls keep working;
- metadata-only focused and file/range `show` callers add `--no-content`;
- one-line iterator `next`/`current` callers add `--no-content`;
- label and `peek --all` defaults remain compact;
- defensive `--no-content` succeeds on every `show` route;
- plumbing remains byte-for-byte unchanged.

Positive content modes still error on covering/connection summaries.
Focused and top-level help must explain both surface classes and their
defaults. No environment or repository setting changes presentation.

## Requirements reconciliation

- **req20 (edit):** add: "Focused `show` views of a single segment or link
  shall include recorded content by default, and spatial views of an
  explicitly selected file or line range shall include the selected source
  segments' recorded content in file order. Each shall accept an explicit
  compact override."
- **req23 (reconcile, no edit):** "without unbounded dumps" forbids implicit
  expansion beyond the user's selector, not complete rendering of an
  explicitly selected file or line range. That selector authorizes its source
  content; `--no-content` requests the compact form. Retain resolved
  relationships and compact label lenses, and do not expand mapped target
  content in a region view.
- **req21 (reconcile, no edit):** frozen task retrievability already supports
  the iterator default.
- **req19 (reconcile, no edit):** plumbing remains line-oriented and
  unchanged.

The asymmetry is deliberate: req20 owns inspection output shape, including
focused and spatial presentation defaults. Req21 defines the iterator model
and guarantees, not cursor presentation, so the iterator default is a
porcelain/interface change reconciled through theory, implementation, skill,
and tutorial.

## Invariants

- Iterator `next`/`current` render frozen object bytes, never a live re-read.
- Existing content-mode mutual exclusion extends to `--no-content`.
- File and line-range views render selected source content in file order and
  retain resolved relationships without expanding target content.
- Label, covering, connection, and `peek` defaults remain compact.
- In `show` views, a content failure never suppresses available metadata or
  relation output; the completed view returns non-zero, except an unavailable
  external target remains successful degraded inspection.
- Iterator retained-object failures preserve the existing warning and cursor
  semantics because `current` is the retry path.
- Plumbing output is byte-for-byte unchanged.
- No ref layout, object layout, iterator storage, or format version changes.

## Alternatives rejected

- **Default label content:** rejected because a semantic label may select an
  open-ended cross-file set whose size is not apparent from the command.
- **Cardinality-gated label content:** rejected because structural edits would
  silently change the same command's output shape.
- **Mapped target content in file/range views:** rejected because it would turn
  a spatial source walkthrough into a potentially much larger two-sided dump.
  Single-link inspection supplies the two-sided content view.
- **Positive content expansion on covering/connection summaries:** rejected
  because existing selectors already provide the drill-down.
- **Live links in iterator output:** rejected because links are not frozen
  iterator state.

## Implementation and tests

Implementation:

- represent content selection as optional CLI override plus route default;
- reuse `src/cli/content.rs` for every block;
- pass content mode into single-link rendering and retain the target `Repo`;
- resolve concrete named endpoints and use source contexts per repository;
- default file and line-range regions to numbered source content while keeping
  label lenses compact;
- retain per-segment content-read failures so region and label renderers can
  print available relationships, continue, and return non-zero afterward;
- default iterator `next`/`current` to numbered and `peek` to compact;
- implement the validation order above.

Tests must cover:

- both surface classes, defaults, explicit modes, and conflicts;
- covering-qualified segment default flip, failure context, and bare-hash
  suppression;
- ordered file/range decomposition, straddling-range intersection,
  relationship retention, source-only content, `--no-content`, and failure
  continuation;
- compact label defaults and explicit-content failure continuation;
- two role-qualified link blocks, duplicate hashes, broken links, and peers;
- iterator frozen bytes, `--no-content`, and compact default `peek`;
- exact summary errors, including `--content --all` precedence and successful
  `--no-content --all`;
- `--all` rejection on every route other than a connection summary;
- unchanged plumbing.

## Landing checklist

- **Theory:** update focused, spatial-region, and iterator presentation
  defaults; retain resolved-region relationships and compact label lenses.
- **Requirements:** apply the req20 sentence above; reconcile req23 using the
  selector-bound interpretation above; confirm req19, req21, and req23 need no
  text edit.
- **Implementation:** update parsing/rendering and add unit/smoke tests.
- **Skill / playground:** make focused and spatial bare commands canonical,
  retain explicit label content, teach `--no-content`, and update T8/T9.
- **Tutorial:** remove redundant flags from focused segment and iterator
  examples, show file decomposition without `--content`, retain explicit
  content on label examples, show a straddling line-range selection, and add
  single-link comparison.
- **README / glossary:** document both surface classes and affected terms.
- **Self-map:** reconcile the req20/theory edits, then run `covmap status` and
  `covmap fsck`.

No format-version bump is required.
