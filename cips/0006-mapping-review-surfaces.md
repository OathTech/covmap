# Mapping review surfaces

Stage: **Draft**

## Relationship to CIP 0009

This proposal remains an active implementation candidate and is independent of
[CIP 0009](0009-atomic-multi-cut.md). Both operate on the existing complete
covering model: CIP 0009 changes refinement, while this proposal changes
read-only inspection.

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

- `ls -l` has a stable fifth name column and prints `@name` or `-`. It still
  has no interactive header.
- `ls` accepts at most one region and returns only its usage string when more
  are supplied.
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
`HEAD`; all remaining positionals are regions. A segment selected by
overlapping regions is printed once. Output remains in covering file/position
order, not argument order.

Errors name the bad region and the accepted grammar rather than falling back
to the whole-command usage line.

### Porcelain column header

When stdout is a terminal, `ls -l` prints:

```text
HASH              LABEL       LOCATION                    LINES  NAME        MESSAGE
```

When redirected or piped it retains today's header-free TAB records for
compatibility. `ls-segments -l` remains the documented machine interface.

### Degree metrics

For a connection relation `kappa`, define:

```text
out-degree(s) = |kappa({s})|
in-degree(t)  = |kappa^-1({t})|
```

Keep `status` unchanged as the terse health summary. `show <K>` adds degree
maxima next to its existing coverage details:

```text
degree max-out 1, max-in 5
```

It also always prints non-trivial hubs, independent of the 30-link dump
threshold:

```text
fan-in:
     5 <- @validate-request  src/api.rs:...
     3 <- @normalize-input   src/api.rs:...
```

Print `fan-out` symmetrically when any source has out-degree greater than one.
The existing threshold still decides whether the individual link list is
shown by default.

### Readable existing connection output

Improve the existing `show <K>` link list, including the list forced by
`show <K> --all`. Print one resolved link heading followed by its note as an
indented, wrapped paragraph:

```text
@parse-record src/parser.rs:... -> @decode-record src/reader.rs:...
  The parser normalizes escaped fields before the reader validates the
  decoded value.
```

Links without notes keep their current one-line form. Preserve embedded
newlines as paragraphs and wrap only porcelain output; `ls-links --resolve`
stays byte-for-byte unchanged. The existing large-connection threshold and
`--all` behavior remain unchanged.

## Surface-area check

This proposal adds no verb or flag. Multi-region `ls` removes an arbitrary
one-region parser limit, while the header, degree summary, hubs, and note
wrapping improve existing output. Existing `ls` filters and `show --all` are
reused rather than duplicated.

## Implementation

- Parse `ls` positionals into one covering plus `Vec<Region>`, using a shared
  region parser currently duplicated between `ls` and `show`.
- Use `std::io::IsTerminal` for the optional header.
- Add a connection-metrics helper returning resolved in/out degree maps for
  `show`; do not change `status` output.
- Add a small fixed-width paragraph wrapper for existing connection-note
  output; do not add a dependency solely for wrapping.
- Keep all changes read-only and format-neutral.

## Tests

- Several file and range regions produce a deduplicated union in covering
  order.
- A malformed region reports that region explicitly.
- TTY and piped `ls -l` differ only by the header.
- A sub-30-link connection with fan-in five reports the hub.
- `status` output remains unchanged and contains no degree detail.
- Broken endpoints are excluded from resolved degree counts and remain in the
  broken count.
- `show <K>` and `show <K> --all` wrap multi-paragraph notes beneath resolved
  endpoint headings; `ls-links --resolve` is unchanged.

## Landing checklist

- **Theory:** add in/out degree as relation measures next to coverage and
  allow a region selector to denote a finite union of file/range regions.
- **Requirements:** amend req12 (degree metrics), req20 (note inspection), and
  req23 (multi-region bounded review and hub summaries).
- **Implementation:** update CLI/rendering helpers and add unit/integration
  tests.
- **Skill:** document multi-file review, hub interpretation, and readable
  `show <K> --all` note review.
- **Tutorial:** review two files in one `ls`, create a small fan-in hub, and
  assert wrapped notes in the existing connection report.
- **README / glossary:** update inspection signatures and define degree/fan-in.
- **Self-map:** remap amended requirements and run `covmap status` plus
  `covmap fsck`.

Before this proposal advances beyond Draft, satisfy the
[baseline prerequisite](README.md#baseline-prerequisite-for-current-drafts).

No format-version bump is required.
