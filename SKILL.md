---
name: covmap
description: Use covmap to partition repositories into content-addressed segments and link segments across two repositories (e.g. a markdown spec and an implementation). Trigger whenever the user asks to map, track, or cover a repository against another, audit which segments are linked, or repair links after one of the repositories was edited. Also trigger when the user pastes `covmap status` or `covmap fsck` output.
---

# covmap skill

covmap is a local CLI that partitions each of two repositories into
content-addressed **segments** organized into named **coverings**,
then records **connections** between segments of the two repositories.
This skill teaches you how to drive it as fluently as `git`.

The mathematical model is in `covmap/theory/main.tex`; the source is
`covmap/src/`. Build with `cargo build --release` from `covmap/`. The
binary path is `covmap/target/release/covmap`.

## Mental model

- A **segment** is a contiguous range of one file `(file, a, b, label)`
  in one repository, displayed as `file:a..b`. `a` and `b` are
  1-indexed line numbers; each may carry an optional 1-indexed
  inclusive character column (`a:col`), so a range can be
  character-precise: `file:5:3..7:9`. An omitted column means the line
  boundary (start of the line for `a`, through the newline for `b`).
  Its identity is the first 16 hex chars (64 bits) of `sha256(content)` —
  content only; the label is ref-local metadata, not part of the hash.
  Segments are stored under `.covmap/objects/`.
- A **covering** is a partition of (some files of) a repository into
  segments. It is a ref under `.covmap/coverings/<name>` listing one
  segment per line, prefixed by `# covered=<file>` lines. The default
  covering is named `main` and is pointed at by `.covmap/HEAD`.
- A **name** is an optional stable identity for a segment (a
  *segment*), set with `covmap name`. It is shown as `@name`. Naming is for
  segments you intend to map: a connection referencing a segment by
  `@name` survives edits to the segment's content, because identity is the
  name, not the hash. Anonymous segments (the default, e.g. `unknown`
  segments) are referenced by segment hash, which changes on every edit.
- A **message** is an optional free-form note *about a segment itself* —
  why it was cut, what it represents, why it is (not) mapped — set with
  `covmap annotate ... -m`. It may be multi-line, is covering-local, and is
  **not** part of the segment hash, so editing it never causes drift; it
  travels with the segment across split/merge/recut/remap like a name does.
- A **connection** is a relation between segments of one covering and
  segments of another. It is a ref under
  `.covmap/connections/<name>`. A connection lives in the source repo
  and references the target repo by absolute path, including for a
  same-repository self-map. Each endpoint is either a segment name
  (`@name`, resilient to content edits) or a segment hash (legacy /
  anonymous). `disconnect` deletes only the connection ref.
- **Segment hash identity is content-only.** Equal content has the same hash
  regardless of label. Labels and names are covering-local metadata and do
  not affect the segment hash.
- **There are no segmentation rules.** `covmap cover` walks the repo
  and adds a single whole-file segment labelled `unknown` for every
  readable UTF-8 file it has not yet covered. The user / agent then
  refines the covering with `covmap split` and `covmap merge`. `split`
  atomically inserts one or more cut positions; it does not set metadata.

### Where text lives — five channels, don't confuse them

covmap has five places text can attach. Pick by *what the text is about*:

| Channel | Attaches to | Purpose | Hashed? | Set with |
| ------- | ----------- | ------- | ------- | -------- |
| **label** | a segment | coarse status *category* (`unknown`/`reviewed`/`mapped`) | no (ref-local) | `covmap label` |
| **name** | a segment | stable *slug identity* (`@enqueue`); no spaces/`:` | no | `covmap name` |
| **message** | a segment | free-form *why this segment* (rationale, provenance, why (un)mapped) | no | `covmap annotate -m` |
| **link note** | a link | *why this mapping* (A↔B) | n/a | `covmap link … <note>` |
| **task note** | an iterator task | *this sweep's verdict* on a segment (transient) | n/a | `covmap iter … done/skip -m` |

Rule of thumb: **message** = a durable fact about the segment; **link
note** = about a correspondence; **task note** = about one pass of work.

## When to use this skill

Use covmap whenever the user asks you to:

- **Map** a markdown spec to one or more code/mechanized
  implementations.
- **Audit coverage**: which segments of repo A are linked, which are
  not, and how meaningful the segment boundaries are.
- **Resolve drift**: repo A was edited; what links broke and how do we
  fix them?
- **Triage** a `covmap status` or `covmap fsck` paste.
- **Set up** covmap on a new pair of directories (init + cover).

Do **not** use covmap to verify that an implementation is
*semantically* correct. covmap tracks links, not meaning.

## Layout cheat sheet

A covmap-managed repository contains:

```
.covmap/
  HEAD                    default covering name (default: main)
  ignore                  optional gitignore-lite patterns
  objects/<hh>/<rest>     content-addressed segment objects
  coverings/<name>        covering refs (one per name)
  connections/<name>      connection refs (one per name)
```

## Core commands

Run from within a covmap-managed directory; covmap walks up to find
`.covmap/`. Hash prefixes (≥2 hex chars) are accepted as long as they
are unique.

| Command | Purpose | Read output? |
| ------- | ------- | ------------ |
| `covmap init` | Create `.covmap/` in cwd. | no |
| `covmap cover [<C>]` | Add a whole-file `unknown` segment for every file not yet covered. **Idempotent and additive — never overwrites refined segments.** | yes — segment count |
| `covmap status [<C>]` | Per-covering summary + drift + per-connection broken count and source/target **coverage**. | **yes** |
| `covmap ls [-l] [<C>] [<region> ...] [--label <l>] [--linked\|--unlinked]` | List segments of `C`, optionally narrowed to the union of several file/range **regions** and intersected with a label or link status. | yes |
| `covmap show <addr> [--content] [--all]` | Print a segment, covering, connection, **region** (`<C>:<file>[:a..b]`), or single link (`<K> <A> <B>`). A segment/link shows its **resolved links** both directions + drift state; a region shows each segment with its mapped image. A connection shows resolved degree hubs and wraps mapping notes. `--label <l> [<C>]` = the **label lens** (one call for a cross-cutting concern). `--content` adds a segment's bytes; `--all` expands a large connection. | yes |
| `covmap grep [-l] [-c <C>] <pattern>` | Segments whose label or content matches. | yes |
| `covmap diff <C1> <C2>` | Added / removed / shared segments. | yes |
| `covmap label <C>:<h> <label>` | Replace a segment's label (ref-local; hash unchanged). | no |
| `covmap name <C>:<h\|@name> <name\|->` | Set, rename, or clear (`-`) a segment's name. A rename does not rewrite connections: read its affected-endpoint diagnostics and execute an emitted repair block when present. | **yes when renaming/clearing** |
| `covmap annotate <C>:<addr> [-m <msg>]...` | Set/replace a segment's free-form **message** (why it was cut / what it is / why it is or isn't mapped). Repeatable `-m` = paragraphs; `-` clears; no `-m` prints it. Not hashed — editing it never causes drift. | yes (read) |
| `covmap split <C>:<segment\|file> <cut> [<cut> ...]` | Atomically insert line or character cuts. A file selector can span several original segments. New fragments are `unknown`; the first keeps the original name/message and later fragments are anonymous. Apply labels/names/messages separately. | yes |
| `covmap merge <C>:<h1> <C>:<h2> [<label>]` | Merge two adjacent segments. The merged segment inherits the single surviving name (left wins if both named). | yes |
| `covmap recut <C>:<h>` | Refresh a segment's hash from current disk bytes (boundaries unchanged). Use after a file edit that did not move line boundaries. A named segment keeps its name, so connections by `@name` stay intact. | yes |
| `covmap recut --remap [<C>[:<addr>]] [--threshold <f>]` | **Diff-remap + fingerprint.** Reconstruct each file from the covering, diff against disk, and re-fit every line-aligned segment's boundaries through the unchanged regions — following inserts/deletes automatically and preserving names. Use for **boundary drift** (lines added/removed). Per-file all-or-nothing. For a moved/rewritten segment it can't place, prints a **propose-only** fingerprint relocation guess (ranked, scored; `--threshold` default 0.4). | **yes** |
| `covmap connect <C_A> [<repo>:]<C_B> [<K>]` | Create an empty connection. Omit `<repo>:` for a same-repository target (`connect main main audit`). | yes |
| `covmap disconnect <K>` | Delete only connection ref `K`; coverings and objects are untouched. Regenerate the default HTML report if warned that it may be stale. | yes |
| `covmap link <K> <A> <B> [<note>]` | Add a link. `<A>`/`<B>` are segment addresses (`@name`, hash prefix, or position); a named segment is stored by `@name` so the link survives content edits. Re-linking an existing pair with a new `<note>` updates the note; a bare re-link (no note) is a no-op that keeps it. | no |
| `covmap unlink <K> <A> <B>` | Remove a link using `@name`, a hash prefix, or a position on each side. | no |
| `covmap prune <K>` | Drop links whose endpoints have disappeared from their coverings. | yes |
| `covmap iter [<name> <verb> ...]` | **Freeze a covering into an ordered, resumable worklist.** Bare `iter` lists all iterators; see "Work a covering as a worklist" below. | yes |
| `covmap fsck` | Verify partitions and connection endpoints. | yes |
| `covmap html [-o <path>] [--covering <C>] [--connection <K>]` | Render a static HTML browser. For humans, not you. Shows one connection at a time: auto-selected when only one exists, otherwise `--connection K` is required (with no connections, shows List/Source of HEAD's covering). `--covering`/`--connection` select what it shows independent of HEAD/cwd; `--connection K` fixes the source to K's source covering and renders K's target side in the Target/Compare views (run it from the connection's *source* repo). | no |

### Plumbing (for scripts and pipelines)

| Command | Purpose |
| ------- | ------- |
| `covmap ls-segments [-l] <C>` | One hash per line; `-l` = full record `hash⇥file⇥a⇥b⇥label⇥name⇥message` (TAB, empty fields for absent name/message). |
| `covmap ls-links [--resolve] <K>` | `<endpoint_A>\t<endpoint_B>[\t<note>]` per line; `--resolve` = `a-label⇥a-loc⇥b-label⇥b-loc⇥note`, endpoints resolved so no hand-join. |
| `covmap rev-parse <addr>` | Resolve any address to its full 16-char hash. |
| `covmap cat-segment <hash>` | Raw segment content to stdout. |
| `covmap hash-segment <file> <a> <b> [-w]` | Hash a candidate segment (content only) without registering it (or with `-w`). |
| `covmap fsck --broken-links` | Print one broken link per line: `<conn>\t<endpoint_A>\t<endpoint_B>`. |

### Address forms

- `<hash>` — full 16-char hex or short prefix (≥2 chars), resolved
  against the object store.
- `<C>:<h>` — segment hash `<h>` inside covering `<C>`.
- `<C>:@<name>` — the segment named `<name>` in covering `<C>`.
- `<C>:<file>:<line>[:<col>]` — the segment of `<file>` that **covers**
  that position (unique by the partition invariant). The friendliest,
  least brittle handle: it depends only on where the text is, not on a
  hash. Commands that take a segment address (`merge`, `recut`, `label`,
  `name`, `show`, `link`, `unlink`, …) accept all three forms.
- `split` additionally accepts an exact covered-file selector. Compare
  `covmap split main:spec.md:40 45` (one selected segment) with
  `covmap split main:spec.md 40 75` (two file-level cuts, possibly in
  different original segments).
- Bare `<C>` (no colon) — covering ref by name. `HEAD` resolves to
  the covering named in `.covmap/HEAD`.

For `link`/`unlink` the two endpoints are resolved against the
connection's **own** source and target coverings (which may live in
different repos), so a bare `@name` / hash / `file:line` is the natural
form — `covmap link K @src-seg @tgt-seg`. A `<C>:` prefix is accepted but
only when `<C>` is that side's covering (it is simply stripped); it cannot
point at some other covering.

## Ignoring files — control what gets covered

**This is the single most important setup step, and it is easy to skip.**
`covmap cover` turns *every* readable file in the repo into an `unknown`
whole-file segment. On a real project that means build output, vendored
dependencies, lockfiles, generated code, fixtures, and docs all land in
the covering as noise. The `unknown` list becomes overwhelming, `status`
and `ls` are unreadable, and the human cannot see the files that actually
matter. **Tight, deliberate control over the tracked file set is what
makes covmap usable — treat it as a precondition, not an afterthought.**

covmap reads gitignore-lite patterns from **`.covmap/ignore`**, one per
line, and applies them while walking during `cover`. Ignored directories
are pruned (covmap never descends into them).

| Pattern | Matches |
| ------- | ------- |
| `*.lock` | any file whose basename matches, at any depth |
| `target/` | the `target` directory and everything under it (trailing `/` = directory) |
| `/CODEOWNERS` | anchored — only `CODEOWNERS` at the repo root |
| `vendor/*.go` | path fragment — `*.go` directly inside any `vendor/` |
| `# comment` | comments and blank lines are skipped |

Globs: `*` and `?` match within a single path component (they do not
cross `/`). Unanchored patterns are tested against the full relative
path, each path component, and any trailing path fragment. A missing
`ignore` file means nothing is ignored.

**Recommended discipline before the first `cover`:**

1. List the repo (`tree` / `ls`) and decide what is *source of truth*
   versus generated/vendored/binary noise.
2. Write `.covmap/ignore` to exclude everything that is not source of
   truth — build dirs (`target/`, `build/`, `dist/`, `node_modules/`),
   lockfiles, minified assets, generated code, large fixtures, `.git`
   is already excluded automatically.
3. *Then* run `covmap cover`. Check the segment count and `covmap ls`:
   if the `unknown` list still contains noise, tighten `ignore` and
   re-cover (cover is additive and idempotent, so add the pattern,
   then refine). It is far cheaper to ignore up front than to prune a
   bloated covering later.

If a user complains the `unknown`/uncovered list is "overwhelming" or
"huge", the fix is almost always a tighter `.covmap/ignore` — diagnose
that first.

## Workflows

### A. Set up a new project

1. Identify the two directories. Ask if unclear.
2. From each, run `covmap init`. **Before covering, write
   `.covmap/ignore`** to exclude build output, vendored deps, lockfiles,
   and generated files (see "Ignoring files" above) — skipping this is
   the most common cause of an unusable, noise-filled covering.
3. From each, run `covmap cover`. Each readable UTF-8 file (minus the
   ignored set) becomes one whole-file `unknown` segment. Binaries are
   skipped with a warning. Inspect `covmap ls`; if noise remains,
   tighten `ignore` and re-cover before refining.
4. Refine the spec covering. Use `covmap ls -l` to see file:line ranges,
   gather the meaningful boundaries (`##` headings, declarations, etc.),
   then insert them in one command: `covmap split main:<file> <cut>...`.
   Cuts may be unsorted; covmap validates the full set before replacing the
   covering ref once. Cuts must be interior: omit a heading at line 1 because
   the file already starts there, and do not pass the position immediately
   after the file. A line cut names the first line below the cut. To cut inside
   a line, pass `<line>:<col>`; the fragment below begins at that 1-indexed
   inclusive character. Every resulting fragment is `unknown`, so apply
   deliberate metadata afterward with `label`, `name`, and `annotate`.
5. Run `covmap status` and report cardinality back.
6. Optionally `covmap connect <C_A> [<repo>:]<C_B> <K>` to create the
   empty connection — but do not start linking yet.

### A2. Understand how a part of the software maps (bounded drill-down)

To understand one region of a mapping, do not dump whole coverings and grep.
Drill down in ≤3 calls, ending on a self-sufficient view:

1. **Orient** — `covmap status` shows the coverings, connections, and each
   connection's coverage (how much is mapped).
2. **Narrow** — `covmap ls -l <C> <region>...` lists the union of several
   files or `file:a..b` ranges in covering order, without duplicates
   (`--unlinked` finds gaps; `--label` intersects the same selection).
   If a filename also names a covering, pass the covering explicitly first,
   such as `covmap ls -l HEAD main` to select file `main` in `HEAD`.
3. **Understand** — `covmap show <C>:<file>[:a..b]` prints each segment in
   the region *with the segment(s) it maps to*, resolved to
   `label loc` — the combined view, in one call. For a single segment,
   `covmap show <C>:@name` shows its links both directions plus drift state.

To review the relation's shape and rationale, run `covmap show <K>`. Read
`max-out` as the widest fan-out and `max-in` as the widest fan-in; the bounded
hub lists identify the segments responsible. Broken links are excluded from
those degree counts. Repeated covering locations with identical content share
one content-hash node and produce one hub row at the first covering-order
location. Mapping notes appear on wrapped lines beneath each resolved pair;
add `--all` when the command suppresses a large link list.

You can open the files yourself, so these views never dump content (use
`--content` if you want a segment's bytes inline).

### A3. Trace a cross-cutting concern (label lens)

When an aspect of understanding is spread across many files, label those
segments with a shared status/tag and retrieve the whole concern in one
call:

```
covmap show --label <l> <C>     # every segment labelled <l>, each with its image
```

Labels are ref-local and non-hashed (since cip 0004), so tagging is free and
never breaks a link. `label` is singular per segment, so it doubles as the
segment's status; pick a labelling scheme that suits the concern you want to
trace.

### B. Resolve drift after a source edit

The two kinds of drift you will see:

- **Segment drift** (bytes inside a segment changed, boundaries
  intact). `covmap status` reports `drift: N file(s) edited`. Fix per
  affected segment with `covmap recut <C>:<h>` to update the hash. If
  the segment is **named** (`@name`), connections referencing it stay
  intact across the recut — no relinking needed. If the segment is
  anonymous (linked by hash), `unlink`/`link` to point connections at
  the new hash.
- **Boundary drift** (lines added/removed inside a segment). `recut`
  will refuse if the segment no longer fits the file. Either use
  `covmap recut --remap` (whole-file diff-remap, which re-fits the
  surviving segments automatically — see workflow E), or `merge` the
  affected adjacent segments and re-`split` at the new line numbers.

Steps:

1. Run `covmap status`. Note the drift line and any broken links.
2. Run `covmap fsck --broken-links` to list links whose endpoints
   disappeared from their covering.
3. For each affected segment: choose `recut` (boundaries fine) or
   `merge` + `split` (boundaries shifted). Read the new bytes with
   `covmap show <C>:<new_hash>` to confirm the implementation still
   matches.
4. Only for **anonymous** (hash-linked) segments: `unlink` the old link,
   `link` the new one. Named (`@name`) segments need no relinking.
5. Run `covmap prune <K>` to drop any genuinely-abandoned links.
6. Confirm with `covmap status` (broken count = 0, drift = clean).

**Prefer naming the segments you map.** Before linking, `covmap name
<C>:<h> <name>` the segment, then `link` it — the link is stored by
`@name` and survives later content edits, so drift repair collapses to a
single `recut`. This is the difference between task T3 (hash link, must
relink) and T5 (named link, recut only).

### B2. Rename or clear a linked segment name

`name` changes one covering only. If a connection ref stored in the current
repository contains exact endpoint `@old`, renaming it makes that stored
endpoint broken; covmap lists the affected connection and side. It never
rewrites the connection behind your back.

For a safe rename, execute the commands between the single repair block's
delimiters exactly as printed. For fan-out, the block adds every replacement
before removing any stale pair, preserving each link note:

```text
--- covmap repair begin ---
covmap link 'K' '@new' '@target' 'existing note'
covmap unlink 'K' '@old' '@target'
--- covmap repair end ---
```

Do not invent a repair when no block is emitted: the far endpoint may already
be broken or the replacement pair may exist. Clearing a name has no generic
replacement, so use the printed `unlink` command if the stale link should be
removed. Connections stored in another repository and pointing inbound cannot
be discovered locally; inspect those source repositories separately.

### E. Absorb boundary drift with diff-remap

When an edit **adds or removes lines** so that segment boundaries move
(boundary drift), don't `recut` each segment by hand (it would capture the
wrong lines) and don't `merge`+`split` the whole region. Run:

```
covmap recut --remap            # whole covering (HEAD), or
covmap recut --remap <C>:<addr> # just the file containing that segment
```

It reconstructs each file from the covering, diffs it against disk, and
re-fits every line-aligned segment's boundaries through the unchanged
regions — following the insertion/deletion automatically. Names (hence
`@name` connections) are preserved, so nothing needs relinking. It is
per-file all-or-nothing: a file whose boundaries land in changed regions
is left untouched and its segments are listed as **unresolved**.
After a successful remap, `status` is clean and `fsck` passes. This is
task T6.

When a file is healed, the output names each segment whose line range
actually moved, `old..b -> new..b`, e.g. `@seg-mid: 3..4 -> 5..6` — so you
can confirm a segment spans the new content without re-reading `ls -l`.

Caveats: diff-remap handles **line-aligned** segments; a *drifted* file
that contains character-level cuts is left untouched (use manual
`merge`/`split` for it). Undrifted files are skipped silently, whatever
their segment shape — so it is safe to run `recut --remap` over a whole
covering even when some files have character-level segments: only the files
you actually edited are considered. And it re-fits boundaries that
survived — a segment whose own anchor lines were rewritten is reported
unresolved rather than guessed.

**Moved or rewritten segments — fingerprint proposals.** When a segment can't
be placed by diff-remap (typically because the segment was *moved*),
`recut --remap` prints a **propose-only** relocation guess for it: the
most similar line range in the new file, with a match score, e.g.

```
@audit-retention: policy.md:13..15  (98% match)  — covmap unlink/link to relocate
```

This changes nothing on its own — a score is a likelihood, not a proof.
**Confirm** the candidate (read those lines; an agent can verify
semantically in one step), then re-cut the segment at its new home (e.g.
`merge`+`split`, then `name` it) so the covering is a clean partition
again. Use `--threshold <f>` (default 0.4) to widen or tighten which
guesses are shown. This is task T7. The three repair layers form a
chain: **name** keeps the link alive → **diff-remap** re-fits
surviving boundaries → **fingerprint** proposes a relocation for the
rest.

### C. Annotate / mark a batch of unlinked segments

covmap labels are ref-local status metadata — *not* part of segment
identity — so relabelling is cheap and safe to do freely, even on linked
segments. Common labels: `unknown` (default for fresh segments),
`ignored`, `reviewed`, `mapped`.

```
# label every unmapped Increment-related segment as 'reviewed'
covmap grep -l Increment | while read h; do
  covmap label "main:$h" reviewed
done
```

Relabeling changes only the covering row; the segment's hash (its content
identity) is unchanged, the object store is untouched, and every
connection endpoint — by hash or by name — keeps resolving.

### D. Triage a `covmap status` paste

`covmap status` lines you may see, and what they mean:

```
covering main: segments=7 lines=29
  drift: 1 file(s) edited, 0 file(s) missing/binary
connections:
  widget_links: main -> /tmp/impl/main : 2 links, 1 broken
```

- `drift: clean` — every covered file's bytes still match the
  recorded segments.
- `drift: N file(s) edited` — N files have been modified since their
  segments were recorded. Use `recut` (or merge+split if boundaries
  moved) to refresh.
- `<conn>: <source-C> -> <repo>/<target-C> : N links, M broken` — relink
  or `prune` if `M > 0`. Same-repository targets display `(self-map)`.

### F. Work a covering as a worklist (iterators)

When you have a covering and a *per-segment job* to do across all of it —
audit every segment, port each chunk, write a test for each uncovered
piece — don't hold the plan in your context and hope you don't drift. An
**iterator** freezes the covering into an ordered, stateful worklist on
disk: each segment becomes a **task** you pull one at a time, work, and
mark `done` or `skipped`. The cursor and per-task state live in
`.covmap/iterators/<name>`, so after a compaction or crash `covmap iter
<name> current` re-establishes exactly where you were. This is why it
matters for you specifically: the plan is durable and resumable, not
context-window-bound.

An iterator is a **snapshot**: `new` records the segments at freeze time,
so later edits to the files or covering don't change the enumeration. And
because the object store is append-only, each frozen task's content is
always recoverable with `covmap cat-segment <hash>` (or `iter ... peek
--content`) even if the covering is later edited or deleted. It never
mutates segments, coverings, or connections.

**Setup — freeze the covering:**

```sh
covmap iter sweep new main --order file        # every segment, reading order
covmap iter backfill new main --label unknown  # only the not-yet-refined segments
```

`--order` is `file` (path, then line) or `label` (group a concern);
`--label <l>` narrows the frozen set to one label — ideal for a backfill.

**The canonical loop** — `next` hands you one task; you resolve it before
asking for the next. Keep the loop body comment-free: an interactive
**zsh** does not treat `#` as a comment, so a pasted `# ...` line becomes a
command and the task never gets resolved (the loop then spins). Do the work
between `next` and `done`:

```sh
covmap iter sweep new main --order file
until covmap iter sweep finished; do
  covmap iter sweep next --content    # advance + read the task's frozen bytes
  covmap iter sweep done -m "verdict"
done
```

Between `next` and `done`, work on the current task: `next --content`
already handed you its frozen bytes (or fetch them any time with `covmap
iter sweep current --content` / `covmap cat-segment <hash>`), edit code, run
tests — whatever the job is. Use `covmap iter sweep skip -m "why"` instead of
`done` to pass on a task deliberately. If you are auditing a *connection*,
`covmap show <C>:@name` is often better than raw content — it shows the
segment together with its resolved links.

Each `next` / `current` / `peek` prints one parseable line per task:

```
<@name|hash>	<label>	<file>:<a>..<b>
```

Add `--content` (to `next`/`current`/`peek`) to also dump the frozen
bytes; by default stdout stays terse so the loop is easy to parse.

**The enumeration guarantee — why you can't lose a task:**

- `next` advances to the **first pending task in the frozen order**
  (globally, not just after the cursor), so nothing is stepped over.
- `next` is **refused while the current task is still pending** — you must
  resolve it (`done` = worked it, `skip` = deliberately passed) before
  advancing. To pass on a task, `skip` it, then `next`.
- `choose <addr>` is the deliberate jump: it moves the cursor (even onto a
  pending task) but leaves every task's state alone, so a later `next`
  comes back to anything still pending. Jumping can't break completeness.
- `finished` is true **only** when no pending task remains (all `done` or
  `skipped`) — the loop condition above.

**Other verbs:** `peek` (list **all pending tasks** in order without
advancing — the set you can `choose` among, including the active one;
`peek --content` also dumps their bytes, `peek --all` lists the whole
worklist tagged with each task's state), `choose <addr>` (jump the cursor
to a task by `@name` or hash prefix — pick one you saw in `peek`), `status`
(`23/40 resolved (20 done, 3 skipped) — active <handle>`, or `— finished`
when nothing is pending — the fraction counts resolved = done+skipped, so
it hits total exactly when the sweep is done), `rewind [<addr>]` (reset a
`done`/`skipped` task back to `pending`), `rm` (delete the iterator — never
touches the covering). This is playground task T8. There is at most one
active task at a time (a single cursor), which is why `status` names it
rather than counting it.

The `-m` note on `done`/`skip` is the task's verdict; each resolution
*replaces* it, so re-resolving a task without `-m` clears any earlier
note (`done -m "verified"` then `skip` leaves no stale "verified").

Iterators are **not** for tracking drift — a frozen task never goes
"stale" (its content is pinned by hash). Drift is a property of the
covering; repair it with `recut` / `recut --remap` (workflows B, E).

## Efficiency rules

- **Batch `split`s** by reading the spec once with `covmap ls -l`,
  picking all your cut lines, then running splits in order.
- **Prefer short hash prefixes** (8 chars is plenty) in commands and
  user-facing output. Use `rev-parse` to disambiguate.
- **Do not edit `.covmap/coverings/*` or `.covmap/connections/*` by
  hand.** Always go through CLI commands.
- **Invoke each plumbing command in a pipeline** instead of stitching
  multiple porcelain commands. Plumbing is line-oriented and stable.
- **Never `cargo build`** in a hot path; the binary at
  `covmap/target/release/covmap` is what you want.

## Error handling

- `covmap: no .covmap directory found...` → run `covmap init` (and
  ask first whether this is the right directory).
- `covmap: no segment matches '<prefix>'` → the prefix is wrong or
  refers to an object that's been removed; widen the prefix or
  `covmap ls -l` to find a current one.
- `covmap: ambiguous: N segments match '<prefix>'` → use a longer
  prefix.
- `covmap: split cut=<n> out of range for segment <file:a..b>` → you
  asked to split outside the segment, or at a position that would create
  an empty fragment; check `covmap ls -l` for the actual range. The cut
  may be a line (`<n>`) or a character position (`<n>:<col>`).
- `covmap: split cut '<n>' is a file boundary...` → file-level cuts must
  be interior. Omit line 1 and the position immediately after the file;
  those boundaries already exist.
- `covmap: invalid split cut '<text>'...` with a metadata hint → `split`
  accepts only cuts after its selector. Apply former inline labels with a
  separate `covmap label` command. A numeric former label is parsed as a cut.

## Minimum-viable session

For a repository containing `spec.md` and `src/lib.rs`:

```
$ covmap init
$ covmap cover
$ covmap status
$ covmap ls -l
$ covmap split main:spec.md 10 25
$ covmap label main:spec.md:1 Intro
$ covmap label main:spec.md:10 Body
$ covmap name main:spec.md:1 intro
$ covmap name main:src/lib.rs:1 implementation
$ covmap connect main main spec_to_impl
$ covmap link spec_to_impl @intro @implementation "fn foo"
$ covmap fsck
```

After any spec edit, run `covmap status`; for each affected segment
choose `recut` or merge+split, then re-link broken links.
