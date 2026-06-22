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
  Its identity is the first 16 hex chars (64 bits) of
  `sha256(label || NUL || content)`. Segments are stored under
  `.covmap/objects/`.
- A **covering** is a partition of (some files of) a repository into
  segments. It is a ref under `.covmap/coverings/<name>` listing one
  segment per line, prefixed by `# covered=<file>` lines. The default
  covering is named `main` and is pointed at by `.covmap/HEAD`.
- A **connection** is a relation between segments of one covering and
  segments of another. It is a ref under
  `.covmap/connections/<name>`. A connection lives in the source repo
  and references the target repo by absolute path.
- **Sharing** is automatic: any two coverings (in any repo) that
  produce a segment with the same content+label end up referencing
  the same object.
- **There are no segmentation rules.** `covmap cover` walks the repo
  and adds a single whole-file segment labelled `unknown` for every
  readable UTF-8 file it has not yet covered. The user / agent then
  refines the covering with `covmap split` and `covmap merge`.

## When to use this skill

Use covmap whenever the user asks you to:

- **Map** a markdown spec to one or more code/mechanized
  implementations.
- **Audit coverage**: which segments of repo A are linked, which are
  not, and how meaningful the segment boundaries are.
- **Resolve drift**: repo A was edited; what links broke and how do we
  fix them?
- **Triage** a `covmap status` or `covmap fsck` paste.
- **Bootstrap** covmap on a new pair of directories.

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
| `covmap status [<C>]` | Per-covering summary + drift + broken-pair count. | **yes** |
| `covmap ls [-l] [<C>]` | List segments of covering `C`. | yes |
| `covmap show <addr>` | Pretty-print a segment, covering, or connection. | yes |
| `covmap grep [-l] [-c <C>] <pattern>` | Segments whose label or content matches. | yes |
| `covmap diff <C1> <C2>` | Added / removed / shared segments. | yes |
| `covmap label <C>:<h> <label>` | Replace a segment's label (re-hashes). | no |
| `covmap split <C>:<h> <line[:col]> [<lbl_top>] [<lbl_bot>]` | Split a segment at `<line>` (the first line of the new bottom segment), or mid-line at `<line>:<col>` (the first character of the new bottom segment). | yes |
| `covmap merge <C>:<h1> <C>:<h2> [<label>]` | Merge two adjacent segments. | yes |
| `covmap recut <C>:<h>` | Refresh a segment's hash from current disk bytes (boundaries unchanged). Use after a file edit that did not move line boundaries. | yes |
| `covmap connect <C_A> <repoB:C_B> [<K>]` | Create empty connection. | no |
| `covmap link <K> <h_A> <h_B> [<note>]` | Add a pair. | no |
| `covmap unlink <K> <h_A> <h_B>` | Remove a pair. | no |
| `covmap repair <K>` | Drop pairs whose endpoints have disappeared from their coverings. | yes |
| `covmap fsck` | Verify partitions and connection endpoints. | yes |
| `covmap html [-o <path>]` | Render a static HTML browser. For humans, not you. | no |

### Plumbing (for scripts and pipelines)

| Command | Purpose |
| ------- | ------- |
| `covmap ls-segments <C>` | One hash per line. |
| `covmap ls-pairs <K>` | `<hash_A>\t<hash_B>[\t<note>]` per line. |
| `covmap rev-parse <addr>` | Resolve any address to its full 16-char hash. |
| `covmap cat-segment <hash>` | Raw segment content to stdout. |
| `covmap hash-segment <file> <a> <b> [<lbl>] [-w]` | Hash a candidate segment without registering it (or with `-w`). |
| `covmap fsck --broken-pairs` | Print one broken pair per line: `<conn>\t<hash_A>\t<hash_B>`. |

### Address forms

- `<hash>` — full 16-char hex or short prefix (≥2 chars), resolved
  against the object store.
- `<C>:<h>` — segment hash `<h>` inside covering `<C>`.
- Bare `<C>` (no colon) — covering ref by name. `HEAD` resolves to
  the covering named in `.covmap/HEAD`.

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

### A. Bootstrap a new project

1. Identify the two directories. Ask if unclear.
2. From each, run `covmap init`. **Before covering, write
   `.covmap/ignore`** to exclude build output, vendored deps, lockfiles,
   and generated files (see "Ignoring files" above) — skipping this is
   the most common cause of an unusable, noise-filled covering.
3. From each, run `covmap cover`. Each readable UTF-8 file (minus the
   ignored set) becomes one whole-file `unknown` segment. Binaries are
   skipped with a warning. Inspect `covmap ls`; if noise remains,
   tighten `ignore` and re-cover before refining.
4. Refine the spec covering. Use `covmap ls -l` to see file:line
   ranges, then `covmap split main:<hash> <line> <lbl_top> <lbl_bot>`
   at each meaningful boundary (`##` headings, function declarations,
   etc.). The first line of the bottom segment is `<line>`. To cut
   inside a line — e.g. to isolate an inline term or a clause — pass
   `<line>:<col>` instead; the bottom segment then begins at character
   `<col>` (1-indexed, inclusive) of that line.
5. Run `covmap status` and report cardinality back.
6. Optionally `covmap connect <C_A> <repoB:C_B> <K>` to create the
   empty connection — but do not start linking yet.

### B. Resolve drift after a source edit

The two kinds of drift you will see:

- **File-level drift** (bytes inside a segment changed, boundaries
  intact). `covmap status` reports `drift: N file(s) edited`. No pair
  is broken yet, because the covering still holds the pre-edit hash.
  Fix per affected segment with `covmap recut <C>:<h>` to update the
  hash, then `unlink`/`link` to point connections at the new hash.
- **Boundary drift** (lines added/removed inside a segment). `recut`
  will refuse if the segment no longer fits the file. You must
  `merge` the affected adjacent segments first, then re-`split` at
  the new line numbers.

Steps:

1. Run `covmap status`. Note the drift line and any broken pairs.
2. Run `covmap fsck --broken-pairs` to list pairs whose endpoints
   disappeared from their covering.
3. For each affected segment: choose `recut` (boundaries fine) or
   `merge` + `split` (boundaries shifted). Read the new bytes with
   `covmap show <C>:<new_hash>` to confirm the implementation still
   matches.
4. `unlink` the old pair, `link` the new one.
5. Run `covmap repair <K>` to drop any genuinely-abandoned pairs.
6. Confirm with `covmap status` (broken count = 0, drift = clean).

### C. Annotate / mark a batch of unlinked segments

covmap labels are part of segment identity, so use them sparingly.
Common labels: `unknown` (default for fresh segments), `ignored`,
`reviewed`, `mapped`.

```
# label every unmapped Increment-related segment as 'reviewed'
covmap grep -l Increment | while read h; do
  covmap label "main:$h" reviewed
done
```

Relabeling rewrites the entry's hash; the old object stays in the
store so any pre-existing connection keeps resolving until the user
explicitly re-links.

### D. Triage a `covmap status` paste

`covmap status` lines you may see, and what they mean:

```
covering main: segments=7 lines=29 entropy=2.802
  drift: 1 file(s) edited, 0 file(s) missing/binary
connections:
  widget_links -> /tmp/impl/main : 2 pairs, 1 broken
```

- `drift: clean` — every covered file's bytes still match the
  recorded segments.
- `drift: N file(s) edited` — N files have been modified since their
  segments were recorded. Use `recut` (or merge+split if boundaries
  moved) to refresh.
- `<conn> -> <repo>/<C> : N pairs, M broken` — repair if `M > 0`.

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
  asked to split outside the segment, or at a position that would make
  one side empty; check `covmap ls -l` for the actual range. The cut
  may be a line (`<n>`) or a character position (`<n>:<col>`).

## Minimum-viable session

```
$ covmap init
$ covmap cover
Covering 'main': 3 segments, 120 lines, entropy 1.485 bits (3 added)
$ covmap status
covering main: segments=3 lines=120 entropy=1.485
  drift: clean
$ covmap ls -l
e76e5429f980abcd  unknown  README.md:1..30   30
38f1a9c45dcd1234  unknown  src/lib.rs:1..50  50
9a0947eeea4ab16c  unknown  spec.md:1..40     40
$ covmap split main:9a0947ee 10 "Intro" "Body"
split 9a0947eeea4ab16c -> 5b3a... + 8c4e...
$ covmap connect main /path/to/impl:main spec_to_impl
$ covmap link spec_to_impl 5b3a 38f1 "fn foo"
$ covmap fsck
fsck: ok
```

After any spec edit, run `covmap status`; for each affected segment
choose `recut` or merge+split, then re-link broken pairs.
