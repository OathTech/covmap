# covmap

**Track which parts of a spec are realized by which parts of an
implementation — content-addressed and drift-aware.**

covmap is a small CLI for cross-referencing two repositories at the
granularity of arbitrary line — or character — ranges. Typical use: a Markdown
specification and the code that implements it; or a paper and a
mechanized proof. You partition each side into named **coverings**
and record **connections** between them. When either side is edited,
covmap tells you which links survived and which need re-checking.

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

**v0.1 — experimental.** The CLI surface and on-disk format are
stable enough to use day-to-day but may change before 1.0. Cooperative
single-writer assumed; concurrent writers from multiple processes are
not supported.

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
  the first 16 hex chars of `sha256(label || NUL || content)`.
- A **covering** of a repository is a partition of (some files of)
  the repository into segments. Refs live at
  `.covmap/coverings/<name>`. The default covering is `main`.
- A **name-tag** is an optional stable identity for a segment (a
  *slot*), set with `covmap name` and shown as `@name`. A connection
  that references a slot by `@name` survives edits to the slot's
  content, because the link identity is the name rather than the hash.
  Anonymous segments are referenced by segment hash (which changes on
  every edit).
- A **connection** is a relation between segments of one covering
  and segments of another, possibly in a different repository. Refs
  live at `.covmap/connections/<name>` in the *source* repo. Each
  endpoint is a slot name (`@name`) or a segment hash.
- Sharing is automatic: any two coverings producing a segment with
  the same content+label end up referencing the same object.

There are no built-in segmentation rules. `covmap cover` walks the
repo and emits a single whole-file segment labelled `unknown` for
every readable UTF-8 file it has not yet covered. You refine the
covering with `covmap split`, `covmap merge`, and `covmap recut`.

For the formal model, see [`theory/main.tex`](theory/main.tex)
([rendered PDF](theory/main.pdf)).

## Worked example

Map a Markdown spec to its Rust implementation:

```sh
# 1. Cover the spec side.
cd spec/
covmap init
covmap cover                          # one whole-file `unknown` segment per file
covmap status
# covering main: segments=1 lines=42 entropy=0.000
#   drift: clean

# 2. Refine: split spec.md into Intro (lines 1-9) and Body (lines 10+).
covmap ls -l                          # find the spec.md hash
# 7f0a8e83a07ef76d  unknown  spec.md:1..42  42
covmap split main:7f0a8e83 10 "Intro" "Body"
# split 7f0a8e83a07ef76d -> 838a4a248219f38c + 8c39d43caabe6fc6
# A cut can also fall mid-line: `covmap split main:<h> 10:5` begins the
# bottom segment at character 5 of line 10 (1-indexed, inclusive).

# 3. Cover the implementation side.
cd ../impl/
covmap init
covmap cover

# 4. Open a connection from spec → impl and link the Body segment.
cd ../spec/
covmap connect main ../impl:main spec_to_impl
covmap link spec_to_impl 8c39d43c <impl_hash> "fn body"

# 5. Edit spec.md. Status now reports drift.
covmap status
# covering main: segments=2 lines=42 entropy=...
#   drift: 1 file(s) edited, 0 file(s) missing/binary
#   spec_to_impl -> .../impl/main : 1 pairs, 0 broken

# 6. Refresh the changed segment in place. The pair tied to its old
#    hash becomes broken; relink it to the new hash.
covmap recut main:8c39d43c            # recompute hash from disk bytes
covmap fsck --broken-pairs
covmap unlink spec_to_impl 8c39d43c <impl_hash>
covmap link   spec_to_impl <new_hash> <impl_hash> "fn body"
```

## Commands

Run `covmap help` for the full reference. Top-level porcelain:

| Command | Purpose |
| --- | --- |
| `init` | Create `.covmap/` in the cwd. |
| `cover [<C>]` | Add one whole-file `unknown` segment per uncovered file. |
| `status [<C>]` | Cardinality, entropy, drift, broken pairs. |
| `ls [-l] [<C>]` | List segments in a covering. |
| `show <addr>` | Pretty-print a segment, covering, or connection. |
| `grep [-l] [-c <C>] <pat>` | Segments whose label or content matches. |
| `diff <C1> <C2>` | Added / removed / shared segments. |
| `label <C>:<h> <label>` | Relabel a segment (re-hashes). |
| `name <C>:<h\|@slot> <tag\|->` | Set, rename, or clear a slot's name-tag. |
| `split <C>:<h> <line[:col]> [<top>] [<bot>]` | Split a segment at a line or character position. |
| `merge <C>:<h1> <C>:<h2> [<label>]` | Merge two adjacent segments. |
| `recut <C>:<h>` | Refresh a segment's hash from current disk bytes. |
| `recut --remap [<C>[:<addr>]] [--threshold <f>]` | Diff-remap: re-fit slots onto an edited file. For a moved/rewritten slot it can't place, prints a propose-only fingerprint relocation guess. |
| `connect <C_A> <repoB:C_B> [<K>]` | Create empty connection. |
| `link <K> <h_A> <h_B> [<note>]` | Add a pair. |
| `unlink <K> <h_A> <h_B>` | Remove a pair. |
| `repair <K>` | Drop pairs whose endpoints have left their coverings. |
| `html [-o <path>]` | Render a static HTML browser. |

Plumbing commands (line-oriented, stable for scripts):
`hash-segment`, `cat-segment`, `ls-segments`, `ls-pairs`, `rev-parse`,
`fsck [--broken-pairs]`.

## Address forms

- `<hash>` — full or short hex prefix (≥2 chars). Resolved against
  the object store.
- `<C>:<h>` — segment hash `<h>` inside covering `<C>`.
- `<C>:@<name>` — the slot named `<name>` in covering `<C>`.
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
  version              # on-disk format version (currently "1")
  HEAD                 # default covering name (one line)
  ignore               # optional gitignore-lite patterns
  objects/<hh>/<rest>  # segment objects: "<label>\0<content>"
  coverings/<name>     # covered files + one segment hash per line
  connections/<name>   # connection metadata + pairs
```

Every command checks `.covmap/version` and refuses to operate on a
repo created by an incompatible covmap version. The format version
is bumped on changes to the segment hash, object encoding, or ref
formats; patch releases never bump it.

## Repository layout

```
covmap/
  Cargo.toml
  LICENSE
  README.md
  DEVELOPER.md         # how to evolve covmap (five-artifact consistency contract)
  SKILL.md             # agent-facing workflow guide
  GLOSSARY.md          # vocabulary across theory / requirements / source
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
label → drift → repair on a small fixture in a temporary directory.

## For agents

[`SKILL.md`](SKILL.md) is a workflow guide written for AI coding
agents (Claude Code's skill mechanism reads its frontmatter, but the
content is plain Markdown). The
[`skill_playground/`](skill_playground/README.md) directory has
fixture-based tasks for evaluating how well an agent drives covmap.

## Contributing / evolving covmap

covmap is described by five artifacts that must be kept consistent with
each other — theory, requirements, implementation + tests, skill, and
tutorial. Any change to one must be reconciled across the rest.
[`DEVELOPER.md`](DEVELOPER.md) is the contract: propagation rules by
change type, the gates to run, and the definition of done. Agents should
also read [`AGENT.md`](AGENT.md).

## License

[Apache-2.0](LICENSE).
