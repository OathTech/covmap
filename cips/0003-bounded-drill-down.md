# Bounded drill-down — inspection for understanding

## Idea

Segments and connections both carry real value to a reader (human or agent): the
segments are the addressable, content-stable units, and the connections relate
them across projects. But *understanding* a piece of software often means reading
a segment **together with** what it maps to — and that combined view is exactly
what covmap makes hardest to reach today. Understanding *how one part of a project
maps to another* takes ~5 commands, two of which dump the whole covering /
connection for the reader to grep, and one of which forces a repo switch to
resolve a bare-hash endpoint. This cip reorganizes inspection around a single,
**testable** guarantee and two supporting rules:

1. **Bounded drill-down (the governing principle).** From a cold start,
   understanding one bounded *region* of the mapping takes **≤ 3 commands**, each
   returning a **bounded** result (never an unbounded dump the reader must
   re-filter), and the **last command is self-sufficient** — it resolves the
   relationship fully, forcing no follow-up lookup.

2. **Two modes (a formatting rule under it).** A command that *lists* many items
   prints a **condensed** form — one line per item, no file content. A command
   that names *one* item prints **all** covmap holds about it. (`git log
   --oneline` vs `git show`.)

3. **Label lens (a cross-cutting entry point).** When an aspect of understanding
   is spread across many files, the reader tags those segments with a shared
   **label**, then retrieves *every segment carrying that label together with its
   mappings* in **one** command. A label turns a scattered concern into a named,
   one-call region. Since cip 0004, labels are non-hashed, ref-local metadata, so
   tagging and re-tagging a segment is free — it never re-hashes it or breaks a
   link, so a lens can be built and refined at will.

Nothing here changes the on-disk format. The whole proposal is **output and
query** over data covmap already stores; the enabling join is code that already
exists inside the HTML renderer, lifted out and exposed to the CLI.

## Why it helps AI agents

An agent reads with a token budget and a short working memory. Unbounded dumps
(a connection with thousands of links prints hundreds of kilobytes) blow the
budget; bare-hash endpoints force blind follow-up calls; "grep the covering
yourself" wastes turns
and invites hallucinated identifiers. A ≤3-call bound with a self-sufficient
final call means an agent can *understand a part of the software* — what it is,
where it lives, and what it corresponds to on the other side — in a predictable,
cheap, and complete interaction. The agent can already open files, so inspection
never re-prints content; it supplies exactly what the agent cannot get from the
files alone: **location + resolved linkage**.

## The three enablers

Bounded drill-down is unreachable today because three primitives are missing.
Each is small; together they are the whole cip.

| Enabler | Today | Needed |
| ------- | ----- | ------ |
| **Region filter** | `ls`/`show` render a whole covering | restrict to a file or a line range within a covering |
| **Endpoint resolution** | links print as bare `hashA hashB` | each endpoint shown as `covering:label loc`, resolved across repos |
| **Segment↔link join** | no segment→links; reader scans connections | from a segment (or region, or label set), list its resolved links, both directions |

The join already exists: `html.rs::render` walks every connection to build
`outgoing` / `incoming` maps and resolves each endpoint via
`covering::endpoint_to_hash` (degrading gracefully when a target covering is
absent). This cip **lifts that into a shared `resolve` module** so the CLI and
the HTML view compute the relation identically.

## Model

No new on-disk state. Two query-time notions:

- **Region** — a slice of a covering: a whole file, or a line range within a
  file. Addressed `<C>:<file>` or `<C>:<file>:a..b`. It is just a filter over the
  covering's entries; it is not stored.
- **Label set** — the segments of a covering whose `label` equals `<l>`. Also a
  query-time filter. Grounded in the theory's connection-as-relation: the label
  lens computes the **image** of a label-selected subset *S ⊆ Cov^A* under the
  connection κ, i.e. κ(S) ⊆ Cov^B, presented as resolved pairs.

**A deliberate scoping choice.** The label lens leverages the *existing* `label`
field (cheap, no format change). Since cip 0004 made `label` non-hashed,
ref-local metadata, tagging is now free — it never re-hashes a segment or breaks
a hash- or name-linked endpoint — so the lens can be built and re-tagged freely.
**One constraint remains:** `label` is **singular** (a segment holds exactly
one), so it doubles as both the segment's status *category* and its lens tag; you
cannot give one segment both a `mapped` status and a `security` theme. A
dedicated multi-valued `tag` channel would lift this, but is a *format change* and
a separate cip — deferred until real use shows the singular limit bite. This cip
is the zero-format-change version.

## Commands / API

The surface is **two verbs** (`ls` lists, `show` renders) over a small set of
selectors. `ls` is always LIST mode; `show`'s mode follows its selector.

### Selectors

| Selector | Names | Mode under `show` |
| -------- | ----- | ----------------- |
| covering | `<C>` | list its segments (LIST) |
| **region** | `<C>:<file>` · `<C>:<file>:a..b` *(new)* | relationship view (LIST of pairs) |
| segment | `<C>:<h>` · `<C>:@name` · `<hash>` · `<file>:line` | all info (DETAIL) |
| connection | `<K>` | list its links (LIST) |
| **link** | `<K> <a> <b>` *(new)* | all info (DETAIL) |
| **label set** | `--label <l> [<C>]` *(new)* | relationship view (LIST of pairs) |

### `ls` — LIST mode (condensed, one line per segment)

```
covmap ls [-l] [<C>] [<file>|<C>:<file>:a..b] [--label <l>]
```

Adds an optional **region** positional and a `--label` filter to today's `ls`.
Long form is `loc  label  hash  name`, name last (it is the one unbounded field —
names can be long, semantic slugs):

```
LOC                     LABEL    HASH              NAME
REQUIREMENTS.md:1..14   req1     c812f565864a42bc  -
REQUIREMENTS.md:15..20  req2     7edaea424c8956b1  -
Cargo.toml:1..22        unknown  4bb144832fba6f8e  -
```

`covmap ls -l main REQUIREMENTS.md` prints only that file's rows — the **narrow**
step. Header is for humans; piped output stays bare TAB (machines parse plumbing,
below). No link column: linkage is a `(covering, connection)` property, not a
covering-local one, so it lives in `show`.

### `show <segment>` — DETAIL mode (all information, resolved links)

```
segment c812f565864a42bc
  covering  main
  loc       REQUIREMENTS.md:1..14   (14 lines)
  label     req1
  name      -
  message   -
  state     current                 (recorded bytes match disk)
  links     1 outgoing · 0 incoming
    -> [reqmap] theory:connection   theory/main.tex:165..178
                links two repositories A and B
```

Everything covmap holds: identity, location, name, message, **drift state**
(names the fix command when stale — the review's "name the next verb" pattern),
and *every* link resolved to the far side's `covering:label loc` with its full
note verbatim on its own line (so parens/pipes in notes never break a parser).
A *target* segment lists its **incoming** links — the reverse index. Content is
omitted by default (the reader can open the file) and added with `--content`.

### `show <region>` and `show --label <l>` — the "understand" call (relationship view)

One renderer, two selectors. Each selected source segment is printed with its
resolved counterpart(s) — the compare view, in text:

```
$ covmap show main:REQUIREMENTS.md:58..61      # the req12 region
region main REQUIREMENTS.md:58..61 · 1 segment · via reqmap

  req12  REQUIREMENTS.md:58..61
    -> entropy    theory/main.tex:225..237   cardinality, granularity, partition entropy
    -> coverage   theory/main.tex:179..190   source/target coverage and totality
```

```
$ covmap show --label <l> main                   # a cross-cutting concern, one call
label "<l>" in main · N segments · via <connection>
  <seg>  <file>:a..b
    -> ...resolved target(s)...
```

This is the payload of the ≤3-call bound: after it, the reader understands the
region/concern and its mapping without another call.

### `show <connection>` — LIST mode, resolved + coverage

Leads with a coverage summary (surfacing the theory-defined ratio the review
found missing), then one resolved line per link; a fan-in summary with `--all`
above a threshold so a large connection is triage, not a dump.

```
connection reqmap   main -> theory   (self-map)
  21 links · source 20/57 segments · target 17/77 segments
  req1   REQUIREMENTS.md:1..14   -> connection   theory/main.tex:165..178   links two repositories…
  …
```

### `show <link>` — DETAIL mode

```
link in reqmap
  note    links two repositories A and B
  status  resolved   (both endpoints present in their coverings)
  source  main : req1        REQUIREMENTS.md:1..14      c812f565864a42bc
  target  theory : connection theory/main.tex:165..178   d71ac0c86f66d177
```

### Orient (call 1) — landscape

`covmap status` gains per-covering and per-connection **coverage** so the first
call already shows where the mapped structure is dense vs. sparse.

### Plumbing (unchanged discipline)

Strict machine output stays in header-free, align-free, TAB plumbing so the
porcelain above can stay human-readable:

```
covmap ls-links <K> --resolve     # a<TAB>a-loc<TAB>b-label<TAB>b-loc<TAB>note
covmap ls-segments -l <C>         # full per-segment record incl. name/label
```

## Invariants

- **The ≤3-call bound holds for any single region or labelled concern.** Warm
  (reader knows the file): 2 calls — `ls -l <C> <file>` → `show <region>`.
  Cold: 3 — `status` → `ls -l <C> <file>` → `show <region>`. Label known: **1** —
  `show --label <l>`.
- **No command in the drill-down returns an unbounded result** without an
  explicit `--all`; the final "understand" call resolves every endpoint it prints
  (or marks it `unavailable` when the target covering is absent), so it is
  self-sufficient.
- **Read-only and format-stable.** Inspection never mutates state; no ref, object
  layout, or `FORMAT_VERSION` changes.
- **One relation, one code path.** The CLI and the HTML view resolve links
  through the same `resolve` module, so they can never disagree.

## Known limit

Connections are stored in the **source** repo, so **incoming** links are
discoverable only from the repo that stores the connection (all self-maps, so
`reqmap` is fully covered). `show <segment>` in a repo that is only a *target* of
some foreign connection cannot enumerate those incoming links locally; it labels
the block honestly (`0 incoming — among connections stored in this repo`). This
mirrors the HTML reverse index's existing blind spot rather than introducing a
new one.

## Use cases

- **Understand a requirement's justification**: `covmap show main:REQUIREMENTS.md:58..61`
  → the theory block(s) that req maps to under `reqmap`, in one self-sufficient
  call.
- **Cross-cutting concern**: give a set of related segments a shared label, then
  `covmap show --label <l>` → the whole concern and its mapped images across
  files, one call.
- **Triage a large connection**: `covmap show <K>` → coverage + fan-in hubs, not
  a full dump; `--all` when you truly want every link.
- **Answer "is this mapped?"**: `covmap show <C>:@seg` → its links (both
  directions) without scanning any connection by hand.

## Landing checklist (six-artifact consistency)

Per `DEVELOPER.md`, this is a **new capability with a behavior change** to
`show`/`ls`/`status` output, not done until reconciled across all six artifacts
(a→b→c→d→e→f):

- **Theory** (`theory/main.tex`): in §Connections, add the **image of a subset
  under a connection** (κ(S) for S ⊆ Cov^A) as the formal basis of the label lens
  and region view; in §Interface, record bounded drill-down as the intended
  inspection access pattern. No change to the segment/covering/connection
  definitions themselves.
- **Requirements** (`REQUIREMENTS.md`): a new **req23** — "inspection supports
  bounded drill-down to understanding" — mapped to the theory blocks via
  `reqmap`; amend **req20** (inspection/visualization) to reference resolved
  links + coverage, and note **req12**'s coverage is now surfaced by `status`.
- **Implementation** (`src/`): new `src/resolve.rs` (the link-join lifted from
  `html.rs`, which is refactored to consume it); region address parsing in
  `parse_addr`; `--label` + region args on `ls`/`show`; resolved-link and
  relationship-view renderers; `show <link>`; coverage in `status`; the
  `ls-links --resolve` / `ls-segments -l` plumbing; `#[cfg(test)]` tests incl. a
  test that asserts the ≤3-call bound on a fixture (command count, not just
  output).
- **Skill** (`SKILL.md` + a new `skill_playground/` task, next free is **T9**):
  document the drill-down workflow and the label lens; add a fixture task that
  grades "understand this region / concern in ≤3 calls."
- **Tutorial** (`tutorial/`): a step in `run_tutorial.sh` (with assertions) that
  drills down to a region and runs a label lens, narrated in `TUTORIAL.md`.
- **`GLOSSARY.md`**: entries for *region*, *label lens*, *resolved link*, and
  *bounded drill-down / two modes*; cross-reference the compare view.
- **`README.md`**: refresh the inspection command summary.

No `FORMAT_VERSION` bump — every change is output/query over existing state. (A
future multi-valued `tag` channel, if pursued, *would* be a format change and a
separate cip.)

## Landing notes (as built)

Landed. One deliberate deviation from the checklist above: **`html.rs` was not
refactored to consume `src/resolve.rs`.** The two already share the *resolution
rule* — both resolve endpoints through `covering::endpoint_to_hash` — so they
cannot disagree about which segment an endpoint denotes, which is the property
that mattered. Forcing html's richer, compare-view-specific link structs
(per-file-key grouping, separate `src_resolved`/`tgt_resolved` flags) onto
`resolve.rs`'s shape would have been churn with real risk to the working HTML
views and no user-visible benefit. `resolve.rs` is therefore the CLI's shared
substrate; if the two ever need to converge further, that is a follow-up cleanup,
not a correctness fix. Everything else landed as specified, gates green, `covmap
fsck` clean on this repo's own `.covmap/`.
```
