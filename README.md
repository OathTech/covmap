# covmap

**A tool to add a layer of understanding over software systems —
coverings of annotated segments, and mappings between them, kept
explicit, persistent, and traceable.**

covmap is a small CLI that lays this layer beside a repository — in
`.covmap/`, never touching the files themselves — by partitioning each
side into **coverings** of content-addressed **segments** and recording
**connections** (the mappings) between them, in one repository or across
two. Typical use: a Markdown spec and the code that implements it,
requirements and the formal model that justifies them, a paper and its
mechanized proof, or a duplication audit within one tree. The layer is
meant to be read, audited, and navigated by a person or an agent alike —
tracking both sides as they evolve, so the correspondence stays
trustworthy rather than going quietly stale.

Coverings and connections live on disk under `.covmap/`, organized
like `git`: there is an object store, refs, and a `HEAD`.

## Why

You have a spec and an implementation, and over time they drift. Code
review tells you *what changed*; covmap tells you *which spec
requirements are still claimed-realized by some piece of code, and
which links broke when something moved*.

Unlike code-search tools (`cscope`, `ctags`, Sourcegraph), covmap
isn't about jumping to definitions — it's about recording an
explicit, durable mapping between two sets of text segments and
detecting when either side has changed underneath the mapping.

## Status

**v0.3 — experimental.** The CLI surface and on-disk format are
stable enough to use day-to-day but may change before 1.0. Cooperative
single-writer assumed; concurrent writers from multiple processes are
not supported.

The on-disk format is **version 2**. A `.covmap/` created by an older
covmap (format 1) is upgraded with `covmap migrate`, which re-keys every
object under the content-only segment hash (see *Layout on disk*).

## Install

Requires a Rust toolchain (1.70+).

```sh
git clone <this repo>
cd covmap
cargo build --release
# binary at: target/release/covmap
```

There are no runtime dependencies beyond `sha2`.

## Mental model

- A **segment** is a contiguous range of a file plus a label and its
  content. The range is given by a start and end position
  `file:a[:col]..b[:col]`, where each `col` is an optional 1-indexed
  inclusive character column (omitted = line boundary). Its identity is
  the first 16 hex chars of `sha256(content)` — content only; the label is
  covering-local metadata and not part of the hash.
- A **covering** of a repository is a partition of (some files of)
  the repository into segments. Refs live at
  `.covmap/coverings/<name>`. The default covering is `main`.
- A **segment name** is an optional stable identity for a segment, set
  with `covmap name` and shown as `@name`. A connection that references a
  segment by `@name` survives edits to the segment's content, because the
  link identity is the name rather than the hash. Anonymous segments are
  referenced by segment hash (which changes on every edit).
- A **segment message** is optional free-form text *about a segment* — why
  it was cut, what it is, why it is (not) mapped — set with `covmap annotate
  … -m`. Like a name it is covering-local and not part of the hash, so
  editing it never causes drift.
- A **connection** is a relation between segments of one covering
  and segments of another, possibly in a different repository. Refs
  live at `.covmap/connections/<name>` in the *source* repo. Each
  endpoint is a segment name (`@name`) or a segment hash.
- A **label** is a per-segment status string (`unknown`, `reviewed`,
  `mapped`, …) set with `covmap label`. Like a name and a message it is
  covering-local and not part of the hash, so relabelling never changes a
  segment's identity or breaks a link.
- Sharing is automatic: any two coverings producing a segment with
  the same content end up referencing the same object.
- An **iterator** freezes a covering into an ordered, stateful worklist
  (`covmap iter <name> new <C>`): each segment becomes a task you pull one
  at a time and mark `done`/`skipped`. It is a resumable, on-disk work
  plan layered over the covering — it never mutates it.

There are no built-in segmentation rules. `covmap cover` walks the
repo and emits a single whole-file segment labelled `unknown` for
every readable UTF-8 file it has not yet covered. You refine the
covering with `covmap split`, `covmap merge`, and `covmap recut`.

For the formal model, see [`theory/main.tex`](theory/main.tex).

## Worked example

Map a Markdown spec to its Rust implementation:

```sh
# 1. Cover the spec side.
cd spec/
covmap init
covmap cover                          # one whole-file `unknown` segment per file
covmap status
# covering main: segments=1 lines=42
#   drift: clean

# 2. Refine: split spec.md into Intro (lines 1-9) and Body (lines 10+),
#    then name the Body segment so links to it survive content edits.
covmap ls -l                          # find the spec.md hash
# 7f0a8e83a07ef76d  unknown  spec.md:1..42  42
covmap split main:7f0a8e83 10 "Intro" "Body"
# split 7f0a8e83a07ef76d -> 838a4a248219f38c + 8c39d43caabe6fc6
# A cut can also fall mid-line: `covmap split main:<h> 10:5` begins the
# bottom segment at character 5 of line 10 (1-indexed, inclusive).
covmap name main:8c39d43c body        # a stable @name handle for the Body

# 3. Cover the implementation side.
cd ../impl/
covmap init
covmap cover

# 4. Open a connection from spec → impl and link the Body segment by name.
cd ../spec/
covmap connect main ../impl:main spec_to_impl
covmap link spec_to_impl @body <impl_hash> "fn body"

# 5. Edit spec.md's Body. Status reports drift, but the link is intact —
#    it references @body, not the (now-changed) content hash.
covmap status
# covering main: segments=2 lines=42
#   drift: 1 file(s) edited, 0 file(s) missing/binary
# connections:
#   spec_to_impl -> .../impl/main : 1 links, 0 broken · coverage source 1/2, target 1/1

# 6. Refresh the changed segment in place. Because the link is by @name,
#    it follows the new content automatically — no relinking needed.
covmap recut main:@body               # recompute hash from disk bytes
covmap fsck                           # fsck: ok
```

## Commands

Run `covmap help` for the full reference. Top-level porcelain:

| Command | Purpose |
| --- | --- |
| `init` | Create `.covmap/` in the cwd. |
| `cover [<C>]` | Add one whole-file `unknown` segment per uncovered file. |
| `status [<C>]` | Cardinality, lines, drift, broken links, per-connection coverage. |
| `ls [-l] [<C>] [<file>[:a..b]] [--label <l>] [--linked\|--unlinked]` | List segments, optionally narrowed to a region, label, or link status. |
| `show <addr> [--content] [--all]` | Print a segment, covering, connection, region (`<C>:<file>[:a..b]`), or link — with links resolved to `label loc`. `--label <l>` = label lens. |
| `grep [-l] [-c <C>] <pat>` | Segments whose label or content matches. |
| `diff <C1> <C2>` | Added / removed / shared segments. |
| `label <C>:<h> <label>` | Relabel a segment (ref-local; hash unchanged). |
| `name <C>:<h\|@name> <name\|->` | Set, rename, or clear a segment's name. |
| `annotate <C>:<addr> [-m <msg>]...` | Set/replace/clear a segment's free-form message (ref-local; not hashed). |
| `split <C>:<h> <line[:col]> [<top>] [<bot>]` | Split a segment at a line or character position. |
| `merge <C>:<h1> <C>:<h2> [<label>]` | Merge two adjacent segments. |
| `recut <C>:<h>` | Refresh a segment's hash from current disk bytes. |
| `recut --remap [<C>[:<addr>]] [--threshold <f>]` | Diff-remap: re-fit segments onto an edited file. For a moved/rewritten segment it can't place, prints a propose-only fingerprint relocation guess. |
| `connect <C_A> <repoB:C_B> [<K>]` | Create empty connection. |
| `link <K> <h_A> <h_B> [<note>]` | Add a link. |
| `unlink <K> <h_A> <h_B>` | Remove a link. |
| `prune <K>` | Drop links whose endpoints have left their coverings. |
| `iter [<name> <verb> ...]` | Freeze a covering into an ordered, resumable worklist; pull tasks one at a time (`next`/`done`/`skip`). Bare `iter` lists all. |
| `html [-o <path>] [--covering <C>] [--connection <K>]` | Render a static HTML browser (one connection; auto-selected when only one exists). |
| `migrate` | Upgrade `.covmap/` on-disk format (1 → 2: content-only segment hash). |

Plumbing commands (line-oriented, stable for scripts):
`hash-segment`, `cat-segment`, `ls-segments`, `ls-links`, `rev-parse`,
`fsck [--broken-links]`.

`covmap html` renders the whole map as a static, self-contained page:

![covmap's HTML report in Compare mode — side A on the left and side B on
the right, linked segments highlighted, with a hovered segment showing its
annotation and its resolved links to the other side](docs/screenshot.png)

*The Compare view puts the two sides of a connection side by side — side A
(source) on the left, side B (target) on the right. Each side's file list
shows how many of its segments are linked; linked segments are highlighted
in the panels; and hovering a segment reveals its annotation and the
segments it maps to on the other side. (List, Source, and Target views
offer other ways in.)*

## Address forms

- `<hash>` — full or short hex prefix (≥2 chars). Resolved against
  the object store.
- `<C>:<h>` — segment hash `<h>` inside covering `<C>`.
- `<C>:@<name>` — the segment named `<name>` in covering `<C>`.
- `<C>:<file>:<line>[:<col>]` — the segment of `<file>` covering that
  position (unique by the partition invariant); a stable, hash-free
  handle. Every command taking a segment address accepts all three forms.
- `HEAD` resolves to the covering named in `.covmap/HEAD`.

## Ignoring files

covmap reads optional gitignore-lite patterns from `.covmap/ignore`,
one per line. Supported syntax:

- `*.go` — matches any `.go` file by name.
- `schema/` — matches the directory `schema/` and everything under it.
- `/CODEOWNERS` — anchored to the repo root.
- `#` lines and blank lines are ignored.

## Layout on disk

```
<repo>/.covmap/
  version              # on-disk format version (currently "2")
  HEAD                 # default covering name (one line)
  ignore               # optional gitignore-lite patterns
  objects/<hh>/<rest>  # segment objects: raw <content> (hash = sha256(content))
  coverings/<name>     # covered files + one segment hash per line
  connections/<name>   # connection metadata + links
  iterators/<name>     # a covering frozen into an ordered, stateful worklist
```

Every command checks `.covmap/version` and refuses to operate on a
repo created by an incompatible covmap version. The format version
is bumped on changes to the segment hash, object encoding, or ref
formats; patch releases never bump it. To upgrade a repo across a bump,
run `covmap migrate` (e.g. format 1 → 2 re-keys every object under the
content-only segment hash).

## Repository layout

```
covmap/
  Cargo.toml
  LICENSE
  README.md
  DEVELOPER.md         # how to evolve covmap (six-artifact consistency contract)
  SKILL.md             # agent-facing workflow guide
  GLOSSARY.md          # cross-cutting vocabulary index
  src/                 # Rust implementation (incl. unit tests)
  theory/main.tex      # mathematical foundation
  tutorial/            # guided end-to-end tour (run_tutorial.sh is an integration test)
  smoke_test/          # end-to-end shell test
  skill_playground/    # self-contained agent evaluation tasks
```

## Verifying a build

```sh
./smoke_test/run.sh
```

Goes through init → cover → split → merge → connect → link → fsck →
label → drift → prune on a small fixture in a temporary directory.

## For agents

[`SKILL.md`](SKILL.md) is a workflow guide written for AI coding
agents (Claude Code's skill mechanism reads its frontmatter, but the
content is plain Markdown). The
[`skill_playground/`](skill_playground/README.md) directory has
fixture-based tasks for evaluating how well an agent drives covmap.

## Contributing / evolving covmap

covmap is described by six artifacts that must be kept consistent with
each other — theory, requirements, implementation + tests, skill, and
tutorial, plus the cross-cutting glossary. Any change to one must be
reconciled across the rest.
[`DEVELOPER.md`](DEVELOPER.md) is the contract: propagation rules by
change type, the gates to run, and the definition of done. Agents should
also read [`AGENT.md`](AGENT.md).

## License

[Apache-2.0](LICENSE).
