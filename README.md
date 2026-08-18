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

**v0.4 — experimental.** The CLI surface and on-disk format are
stable enough to use day-to-day but may change before 1.0. Cooperative
single-writer assumed; concurrent writers from multiple processes are
not supported.

The on-disk format is **version 3**. `covmap migrate` upgrades format 1
or 2 repositories, preserving content-only hashes while replacing connection
checkout paths with portable peer aliases and canonicalizing tracked refs.
CIP 0010 and req24 define the Git-trackable project artifact.

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
- `.covmap/objects/` is the local content cache. A missing live object is
  reconstructible when the checked-out range still hashes to its covering
  ref; `hydrate` restores it atomically and rejects drifted source. Logical
  hash lookup also searches live covering refs, so clearing the cache does not
  make a clean live segment unaddressable.
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
  endpoint is a segment name (`@name`) or a segment hash. `connect`
  creates the ref; `disconnect` removes only that ref, leaving coverings,
  objects, and generated reports untouched. External refs store a stable peer
  alias; `.covmap/local/peers` binds that alias to a checkout path locally.
  The reserved alias `.` denotes the current repository.
- A **label** is a per-segment status string (`unknown`, `reviewed`,
  `mapped`, …) set with `covmap label`. Like a name and a message it is
  covering-local and not part of the hash, so relabelling never changes a
  segment's identity or breaks a link.
- Sharing is automatic: any two coverings producing a segment with
  the same content end up referencing the same object.
- An **iterator** freezes a covering into an ordered, stateful worklist
  (`covmap iter <name> new <C>`): each segment becomes a task you pull one
  at a time and mark `done`/`skipped`. It is a resumable, on-disk work
  plan layered over the covering — it never mutates it. Creation retains every
  selected task object; a missing clean object is reconstructed, while a
  drifted cache miss refuses the freeze with restore-and-hydrate guidance.

There are no built-in segmentation rules. `covmap cover` walks the
repo and emits a single whole-file segment labelled `unknown` for
every readable UTF-8 file it has not yet covered. You refine the
covering with `covmap split`, `covmap merge`, and `covmap recut`.
`split` inserts one or more cut positions atomically; labels, names,
and messages are applied afterward as separate metadata operations.

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

# 2. Refine: atomically split spec.md into Intro (lines 1-9) and Body
#    (lines 10+), then apply metadata explicitly. Every new fragment
#    starts as unknown.
covmap split main:spec.md 10
# split 2f69d21281b3c270 spec.md:1..42 ->
#   3daa6fe23aac4c3d  unknown  spec.md:1..9
#   be498b0b3d8e19b2  unknown  spec.md:10..42
covmap label main:spec.md:1 Intro
covmap label main:spec.md:10 Body
# A cut can also fall mid-line: `covmap split main:<h> 10:5` begins the
# following fragment at character 5 of line 10 (1-indexed, inclusive).
covmap name main:spec.md:10 body       # a stable @name handle for the Body

# 3. Cover the implementation side.
cd ../impl/
covmap init
covmap cover

# 4. Open a connection from spec → impl and link the Body segment by name.
cd ../spec/
covmap peer bind impl ../impl
covmap connect main impl:main spec_to_impl
covmap link spec_to_impl @body <impl_hash> "fn body"

# 5. Edit spec.md's Body. Status reports drift, but the link is intact —
#    it references @body, not the (now-changed) content hash.
covmap status
# covering main: segments=2 lines=42
#   drift: 1 file(s) edited, 0 file(s) missing/binary
#     edited: spec.md
# connections:
#   spec_to_impl: main -> impl:main : 1 links, 0 broken · coverage source 1/2, target 1/1

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
| `hydrate [<C>]` | Reconstruct missing live cache objects from source that matches the covering hash. |
| `status [<C>]` | Cardinality, lines, drift, broken links, per-connection coverage. |
| `ls [-l] [<C>] [<region> ...] [--label <l>] [--linked\|--unlinked]` | List segments, optionally narrowed to the union of several file/range regions and intersected with a label or link status. |
| `show <addr> [--content] [--all]` | Print a segment, covering, connection, region (`<C>:<file>[:a..b]`), or link — with links resolved to `label loc`. Connection views report resolved degree hubs and wrap notes; `--label <l>` = label lens. |
| `grep [-l] [-c <C>] <pat>` | Segments whose label or content matches. |
| `diff <C1> <C2>` | Added / removed / shared segments. |
| `label <C>:<h> <label>` | Relabel a segment (ref-local; hash unchanged). |
| `name <C>:<h\|@name> <name\|->` | Set, rename, or clear a segment's name. A rename lists exact local `@old` endpoints and emits a safe link-then-unlink repair when possible; it never rewrites connections implicitly. |
| `annotate <C>:<addr> [-m <msg>]...` | Set/replace/clear a segment's free-form message (ref-local; not hashed). |
| `split <C>:<segment\|file> <cut> [<cut> ...]` | Atomically insert one or more line or character cuts. New fragments are `unknown`; apply metadata separately. |
| `merge <C>:<h1> <C>:<h2> [<label>]` | Merge two adjacent segments. |
| `recut <C>:<h>` | Refresh a segment's hash from current disk bytes. |
| `recut --remap [<C>[:<addr>]] [--threshold <f>]` | Diff-remap: re-fit segments onto an edited file. For a moved/rewritten segment it can't place, prints a propose-only fingerprint relocation guess. |
| `peer bind <alias> <path>` / `peer ls` / `peer rename <old> <new>` / `peer unbind <alias>` | Manage checkout-local paths for stable connection peer aliases. |
| `connect <C_A> [<peer>:]<C_B> [<K>]` | Create an empty connection; omit `<peer>:` for another covering in the same repository. |
| `disconnect <K>` | Delete only the named connection ref. |
| `link <K> <A> <B> [<note>]` | Add a link; each endpoint accepts `@name`, a hash prefix, or `file:line[:col]`. |
| `unlink <K> <A> <B>` | Remove a link using the same endpoint forms. |
| `prune <K>` | Drop links whose endpoints have left their coverings. |
| `iter [<name> <verb> ...]` | Freeze a covering into an ordered, resumable worklist; pull tasks one at a time (`next`/`done`/`skip`). Bare `iter` lists all. |
| `html [-o <path>] [--covering <C>] [--connection <K>]` | Render a static HTML browser (one connection; auto-selected when only one exists). |
| `migrate` | Upgrade `.covmap/` on-disk format (1 or 2 → 3). |

Plumbing commands (line-oriented, stable for scripts):
`hash-segment`, `cat-segment`, `ls-segments`, `ls-links`, `rev-parse`,
`fsck [--broken-links] [--offline [--hydrate]]`.

Several regions can be reviewed in one covering-ordered listing:

```sh
covmap ls -l main src/api.rs src/worker.rs:40..120
```

If a region filename also names a covering, make the covering explicit; for
example, `covmap ls -l HEAD main` selects the file `main` in `HEAD`.

The regions form a union; `--label`, `--linked`, and `--unlinked` further
filter that selection. `show <connection>` reports resolved maximum in/out
degree and bounded fan-in/fan-out hubs. Hash and `@name` endpoints resolving
to the same content hash count as one node; repeated covering locations with
that hash produce one hub row at the first covering-order location. Broken
links do not contribute.

Long link notes are wrapped beneath their resolved endpoint pair;
`ls-links --resolve` remains the stable TAB-delimited form for scripts.

### Live object cache

Covering refs record each source range and its content hash. If a live object
is missing, verification reads that range and accepts the cache miss only when
the bytes reproduce the recorded hash:

```sh
covmap fsck --offline            # verify without filling the cache
covmap hydrate main              # fill one covering
covmap fsck --offline --hydrate  # verify and fill every local covering
```

Hydration never changes refs. A present object whose content does not hash to
its filename is corruption. Neither `hydrate` nor
`fsck --offline --hydrate` overwrites a corrupt object; remove that cache file,
then hydrate it again from matching source. A valid cached object does not make
an unavailable covered source healthy: source remains part of the live
covering, so `fsck` fails until it is restored or the covering is changed. A
missing object plus drifted source is unrecoverable from the current checkout,
so covmap refuses to substitute those edited bytes. `split` and `merge` use the
same checked reconstruction rule when their recorded inputs are absent.

Normal inspection also reconstructs checked live bytes. `status`, `show`, and
`ls` opportunistically warm missing selected objects; `grep`, `cat-segment`,
and HTML generation use the same source-backed read path. Cache publication is
best-effort, so inspection still succeeds with a read-only cache, but drift,
unavailable source, and corrupt objects are reported. `fsck` does not populate
the cache unless `--hydrate` is present.

`fsck --broken-links --offline` keeps the existing three-column broken-link
schema. For an external connection, such a row asserts only that the local
source endpoint is broken; stderr reports that the target endpoint was not
checked.

### Split grammar migration

`split` no longer accepts optional top/bottom labels. Every operand after the
selector is a `line[:column]` cut, and one file-addressed command may insert
several cuts atomically:

```sh
covmap split main:spec.md 10 24 41
covmap label main:spec.md:10 reviewed
```

An alphabetic former label now fails with a hint to use `label`, `name`, or
`annotate`. A position-shaped former label such as `42` is indistinguishable
from a cut and is therefore treated as one. This is a command-grammar change
only: it does not alter the format-3 ref schemas.

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

- `<hash>` — full or short hex prefix (≥2 chars). Resolved over distinct
  hashes in all local covering refs plus retained cache-only objects; repeated
  references to one full hash count once.
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

This covering-input policy is distinct from `.covmap/.gitignore`, which
controls the default Git publication boundary. `covmap init` creates:

```gitignore
# Local and reconstructible covmap state.
# Commit .covmap/coverings, .covmap/connections, and covmap metadata.
/local/
/objects/
/iterators/
/*.html
*.tmp
```

Ordinary `git add .covmap` therefore stages portable refs and configuration,
not caches, checkout-specific peer paths, iterator state, reports, or
temporary files. This is a default policy, not an enforcement fence:
`git add -f` can override it and ignore rules do not untrack files already in
the index.

### Publish and restore a mapping

```sh
# Author checkout
covmap peer bind impl ../impl
covmap connect main impl:main spec_to_impl
git add .covmap
git commit

# A clone in a different directory layout
covmap peer ls                         # impl is unbound
covmap fsck --offline --hydrate       # local refs/source only
covmap peer bind impl ../deps/implementation
covmap fsck                           # full relation verification
```

Binding or hydrating changes only ignored local state; neither operation
rewrites the committed connection. When a peer is unbound, `status`, `show`,
and resolved listings preserve source-side information and mark the target
unchecked. Linking, pruning, HTML target rendering, and full `fsck` require a
valid binding.

If local state was tracked before this policy existed, remove it from the
index without deleting the working files:

```sh
git rm -r --cached --ignore-unmatch \
  .covmap/objects .covmap/iterators .covmap/local
```

Teams may opt into cache warming after Git updates with the same body in
`.git/hooks/post-checkout` and `.git/hooks/post-merge`:

```sh
#!/bin/sh
covmap fsck --offline --hydrate
```

covmap does not install hooks or write `.git/`.

## Layout on disk

```
<repo>/.covmap/
  .gitignore           # tracked default publication policy
  version              # on-disk format version (currently "3")
  HEAD                 # default covering name (one line)
  ignore               # optional gitignore-lite patterns
  objects/<hh>/<rest>  # local cache: raw <content> (hash = sha256(content))
  coverings/<name>     # covered files + one segment hash per line
  connections/<name>   # portable peer/covering metadata + links
  local/peers          # ignored checkout-local peer bindings
  iterators/<name>     # a covering frozen into an ordered, stateful worklist
```

Every command checks `.covmap/version` and refuses to operate on a
repo created by an incompatible covmap version. The format version
is bumped on changes to the segment hash, object encoding, or ref
formats; patch releases never bump it. To upgrade a repo across a bump,
run `covmap migrate`. Format 1 first re-keys objects under the content-only
hash; format 2 then stages canonical refs, converts target paths to peer
aliases, records local bindings, and publishes format 3 with the version file
last.

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

Goes through init → cover → split → merge → connect → link → multi-region and
degree review → fsck → label → drift → prune → rename repair → disconnect on
a small fixture in a temporary directory.

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
