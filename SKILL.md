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

- A **segment** is a contiguous range of lines `(file, a, b, label)`
  in one repository. Its identity is the first 16 hex chars (64 bits)
  of `sha256(label || NUL || content)`. Segments are stored under
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
| `covmap split <C>:<h> <line> [<lbl_top>] [<lbl_bot>]` | Split a segment at `<line>` (the first line of the new bottom segment). | yes |
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

## Workflows

### A. Bootstrap a new project

1. Identify the two directories. Ask if unclear.
2. From each, run `covmap init` and `covmap cover`. Each readable
   UTF-8 file becomes one whole-file `unknown` segment. Binaries are
   skipped with a warning.
3. Refine the spec covering. Use `covmap ls -l` to see file:line
   ranges, then `covmap split main:<hash> <line> <lbl_top> <lbl_bot>`
   at each meaningful boundary (`##` headings, function declarations,
   etc.). The first line of the bottom segment is `<line>`.
4. Run `covmap status` and report cardinality back.
5. Optionally `covmap connect <C_A> <repoB:C_B> <K>` to create the
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
- `covmap: split cut=<n> out of range (a, b]` → you asked to split
  outside the segment; check `covmap ls -l` for the actual range.

## Minimum-viable session

```
$ covmap init
$ covmap cover
Covering 'main': 3 segments, 120 lines, entropy 1.485 bits (3 added)
$ covmap status
covering main: segments=3 lines=120 entropy=1.485
  drift: clean
$ covmap ls -l
e76e5429f980abcd  unknown  README.md:1-30   30
38f1a9c45dcd1234  unknown  src/lib.rs:1-50  50
9a0947eeea4ab16c  unknown  spec.md:1-40     40
$ covmap split main:9a0947ee 10 "Intro" "Body"
split 9a0947eeea4ab16c -> 5b3a... + 8c4e...
$ covmap connect main /path/to/impl:main spec_to_impl
$ covmap link spec_to_impl 5b3a 38f1 "fn foo"
$ covmap fsck
fsck: ok
```

After any spec edit, run `covmap status`; for each affected segment
choose `recut` or merge+split, then re-link broken pairs.
