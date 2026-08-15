# Mapping review surfaces

Stage: **Accepted**

## Relationship to CIPs 0007 and 0009

This proposal remains an active implementation candidate and is independent of
[CIP 0009](0009-atomic-multi-cut.md). Both operate on the existing complete
covering model: CIP 0009 changes refinement, while this proposal changes
read-only inspection.

[CIP 0007](0007-connection-lifecycle-workflows.md) has already formalized
same-repository routes and endpoint forms. This proposal preserves those
conventions and changes only how existing connections are summarized and
rendered.

## Idea

Make the existing relation legible at review time: accept several regions in
one listing, surface fan-in/fan-out even on medium-sized connections, and
format existing connection listings readably when links carry notes. Preserve
plumbing output as the stable machine interface.

## Motivation

Reviewing a connection commonly spans several files, but `ls` accepts only
one region. Medium-sized relations can contain meaningful fan-in or fan-out
hubs below the current summary threshold, and inline notes become difficult to
read on already-long resolved link rows. These are inspection problems and
should be solved without changing stored state or plumbing output.

## Current behavior

- `ls` accepts at most one region and returns only its usage string when more
  are supplied.
- A malformed, reversed, zero-bound, or unknown single region can silently
  produce no rows or an unintended whole-file match.
- `show <K>` summarizes fan-in only when a connection has more than 30 links.
  A 21-link relation with a five-way target hub therefore prints every link
  but no degree summary.
- `show <segment>` and `show <K> <A> <B>` expose notes, but
  `show <K> --all` keeps each note on the link's already-long line.

## Command surface

### Multi-region `ls`

Extend `ls` to accept a union of regions:

```text
covmap ls [-l] [<C>] [<region> ...]
  [--label <label>] [--linked|--unlinked]
```

`--label`, `--linked`, and `--unlinked` are existing filters and remain
unchanged. This proposal only generalizes the positional region cardinality
from zero-or-one to zero-or-more.

Examples:

```sh
covmap ls -l main src/api.rs src/worker.rs
covmap ls -l main src/api.rs:40..120 src/storage.rs
```

The first positional is a covering only when it names an existing covering or
`HEAD`; all remaining positionals are regions. To select a region whose name
also names a covering, pass `HEAD` (or another covering) explicitly first.

Parse each region against the selected covering by trying an exact covered
filename first, then an optional rightmost `:a..b` suffix. This preserves
filenames containing `:`. This ordering is intentionally region-specific:
unlike CIP 0009's `split` selector, a region has no `file:line[:column]`
segment-address form competing with an exact filename. Range bounds are
positive and ordered (`a <= b`). An unknown file, malformed range, or invalid
bound names the offending region and the accepted grammar rather than falling
back to the whole-command usage line.

The regions form a union. A segment selected by overlapping regions is printed
once; `--label`, `--linked`, and `--unlinked` intersect that union. Output
remains in covering file/position order, not argument order. Linkedness is
evaluated against the complete stored relation, then combined with the region
and label filters.

### Degree metrics

For a connection relation `kappa`, define:

```text
out-degree(s) = |kappa({s})|
in-degree(t)  = |kappa^-1({t})|
```

Operational degree counts include only stored links whose source and target
endpoints both resolve in the current coverings. Broken links remain reflected
in the existing broken-link diagnostics and contribute to neither degree map.
Key both maps by resolved segment hash, not stored endpoint spelling, so hash
and `@name` endpoints that resolve to the same segment contribute to one node.
Repeated covering entries with the same content hash likewise form one
operational node. Render that node once, at its first covering-order location,
matching existing endpoint display behavior.
An empty resolved relation has maxima zero. If the target covering is
unavailable, report that degree metrics are unavailable instead of presenting
zero maxima as a measured result.

Keep `status` unchanged as the terse health summary. `show <K>` adds degree
maxima next to its existing coverage details:

```text
degree max-out 1, max-in 5
```

It also prints the top non-trivial hubs, independent of the 30-link dump
threshold:

```text
fan-in:
     5 <- @validate-request  src/api.rs:...
     3 <- @normalize-input   src/api.rs:...
  ... 4 more fan-in hubs
```

Print at most ten fan-in and ten fan-out hubs. A hub has degree greater than
one. Rank by descending degree, then by the endpoint's covering order; report
the number omitted when more than ten qualify. Print `fan-out` symmetrically
when any source has out-degree greater than one. The existing threshold still
decides whether the individual link list is shown by default.

### Readable existing connection output

Improve the existing `show <K>` link list, including the list forced by
`show <K> --all`. Print one resolved link heading followed by its note as an
indented paragraph wrapped to a fixed 79-character line width:

```text
@parse-record src/parser.rs:... -> @decode-record src/reader.rs:...
    The parser normalizes escaped fields before the reader validates the
    decoded value.
```

Every noted link uses this heading-plus-note form, regardless of note length;
links without notes keep their current one-line form. Preserve embedded note
text while wrapping on whitespace; link notes are single-line values in the
existing line-oriented connection format. This proposal does not add
multi-line link-note storage. Wrap only porcelain output;
`ls-links --resolve` stays byte-for-byte unchanged. The existing
large-connection threshold and `--all` behavior remain unchanged.

## Surface-area check

This proposal adds no verb or flag. Multi-region `ls` removes an arbitrary
one-region parser limit, while the degree summary, hubs, and note wrapping
improve existing output. Existing `ls` filters and `show --all` are reused
rather than duplicated.

## Implementation

- Replace the divergent `ls` and `show` region parsers with one
  exact-file-first parser, and parse `ls` positionals into one covering plus
  `Vec<Region>`.
- Resolve a connection's links once inside `show_connection`, then derive
  coverage, resolved in/out degree maps, hubs, and link rows from that local
  representation. Reuse existing endpoint resolution and display helpers;
  do not bend the per-covering, multi-connection `resolve::Relation` into a
  per-connection view.
- Add a small fixed-width paragraph wrapper for existing connection-note
  output; do not add a dependency solely for wrapping.
- Keep all changes read-only and format-neutral.

## Tests

- Several file and range regions produce a deduplicated union in covering
  order.
- Region filters intersect label/link filters.
- Exact filenames containing `:` work; malformed, unknown, reversed, and
  zero-bound regions report the offending token explicitly.
- A sub-30-link connection with fan-in five reports the hub.
- Hash and `@name` spellings resolving to one segment produce one combined
  hub whose degree agrees with coverage.
- More than ten hubs are capped, deterministically ordered, and report the
  omitted count.
- `status` output remains unchanged and contains no degree detail.
- A link with either endpoint broken is excluded from resolved degree counts
  and remains in the broken count.
- An unavailable target covering reports unavailable degree metrics and no
  hubs.
- `show <K>` and `show <K> --all` wrap long single-line notes beneath resolved
  endpoint headings; `ls-links --resolve` is unchanged.

## Landing checklist

- **Theory:** add in/out degree as relation measures next to coverage and
  allow a region selector to denote a finite union of file/range regions.
- **Requirements:** amend req12 (degree metrics), req20 (note inspection), and
  req23 (multi-region bounded review and hub summaries).
- **Implementation:** update CLI/rendering helpers and add unit/integration
  tests.
- **Smoke tests:** cover multi-region listing, resolved-identity hubs, bounded
  hub output, wrapped notes, and unchanged plumbing output.
- **Skill:** document multi-file review, hub interpretation, and readable
  `show <K> --all` note review; extend T9 bounded drill-down rather than
  adding an overlapping playground task.
- **Tutorial:** review two files in one `ls`, create a small fan-in hub, and
  assert wrapped notes in the existing connection report.
- **README / glossary:** update inspection signatures and define degree/fan-in.
- **Self-map:** remap amended requirements and run `covmap status` plus
  `covmap fsck`.

No format-version bump is required.
