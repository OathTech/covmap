# Git-tracked coverings and mappings

Stage: **Accepted**

Implementation status: **in progress**. The source-backed cache and live-object
read paths are implemented. Format 3, peer aliases and bindings, generated Git
policy, portable connection refs, and resumable format-2 migration are not yet
implemented; this proposal is not Landed.

## Relationship to landed CIPs

This proposal builds on the command and model changes already landed in
[CIP 0006](0006-mapping-review-surfaces.md),
[CIP 0007](0007-connection-lifecycle-workflows.md), and
[CIP 0009](0009-atomic-multi-cut.md).

- CIP 0007's bare-covering self-map syntax and `disconnect` lifecycle remain
  intact. Peer aliases replace only the external repository component.
- CIP 0006's TAB-delimited `ls-links` schemas remain the plumbing interface;
  format 3 gives their rows canonical relation order.
- CIP 0009's refinement invariant remains intact: `split` reads recorded bytes,
  lazily hydrating them only from source that reproduces the recorded hash.

## Requirement ownership

This CIP is the design and implementation vehicle for
[`req24 — Git-trackable coverings and mappings`](../REQUIREMENTS.md).
req24 owns the end-to-end user capability and the decisions that make it
coherent: deterministic portable refs, the project-artifact/local-state
boundary, peer aliases and local bindings, source-backed hydration, the
generated default ignore policy, and Git independence.

The supporting requirements retain their narrower ownership:

- req15 owns source, cache, and endpoint integrity verification;
- req17 owns the plain-text, versioned on-disk substrate;
- req2 and req10 own connection lifecycle and relation semantics;
- req19 and req20 own stable plumbing and human inspection surfaces; and
- req21 owns local iterator snapshot durability.

Satisfying those requirements independently is necessary but not sufficient
for req24: the tracked artifact is useful only when their guarantees compose
across clones, paths, caches, and peer bindings.

## Idea

CIP 0010 makes a repository's coverings and connections first-class project
artifacts that can be committed beside the files they describe. A fresh
`covmap init` shall produce a Git-ready `.covmap/`: durable state is plain,
deterministic, and portable, while checkout-specific state is ignored by a
`.covmap/.gitignore`.

The durable artifact consists of covering and connection refs plus their
project-level configuration. The content-addressed object store remains a local
cache: live objects can be reconstructed from the checked-out source at the
coordinates and hashes recorded in a covering. This keeps Git history focused
on project knowledge instead of accumulating a second, sharded copy of every
covered byte. covmap does not stage or commit anything; ordinary Git commands
remain the publication mechanism.

Cross-repository mappings need one additional change. A connection currently
stores the target repository's absolute path, which bakes one machine's
checkout layout into the artifact. This cip replaces that path with a stable,
project-local **peer alias**. An ignored local binding maps the alias to a
checkout path on each machine.

## Motivation

A carefully refined covering records project knowledge: semantic boundaries,
stable names, labels, and the rationale in segment messages. A connection adds
another valuable artifact: an explicit, reviewable relation between those
regions. Keeping that state only in one working copy makes it easy to lose and
prevents it from participating in normal project workflows:

- a clone does not receive the covering or mapping;
- a branch cannot evolve its map with its source;
- a review cannot inspect mapping changes beside code changes;
- CI cannot verify that the committed map still describes the committed tree;
- an absolute target path makes a cross-repository connection non-portable even
  if `.covmap/` is manually added to Git.

Git already supplies history, branching, review, distribution, and recovery.
covmap should make its meaningful state suitable for that substrate rather than
build a second publication mechanism.

## Scope and non-goals

This cip makes covmap state **Git-trackable**, not Git-dependent.

- covmap never runs `git add`, `git commit`, or changes `.git/`.
- covmap continues to work outside a Git repository.
- Git revisions are not embedded in connections. A commit naturally versions
  the local covering and connection ref; coordinating commits across two
  repositories remains a project/release concern.
- There is no custom Git merge driver. Refs remain ordinary text and conflicts
  are resolved with normal Git tools, followed by `covmap fsck`.
- The object cache and iterator progress are not project artifacts by default.
  A team that deliberately publishes frozen worklists may opt in to tracking
  both together.
- Garbage collection of the local append-only object cache is out of scope.

Peer aliases and Git publication are logically independent capabilities, but
this cip lands them together because a committed cross-repository connection
is not portable without both, and one format bump/migration is preferable to
two.

## Artifact boundary

`covmap init` creates this layout:

```text
<repo>/.covmap/
  .gitignore             tracked default ignore policy for local state
  version                tracked on-disk format version
  HEAD                   tracked default covering
  ignore                 tracked covering input policy, when present
  objects/<hh>/<rest>    ignored reconstructible content cache
  coverings/<name>       tracked covering refs
  connections/<name>     tracked connection refs
  local/
    peers                ignored checkout-specific peer bindings
  iterators/<name>       ignored work-session state by default
  report.html            ignored generated default HTML report
```

The generated `.covmap/.gitignore` is:

```gitignore
# Local and reconstructible covmap state.
# Commit .covmap/coverings, .covmap/connections, and covmap metadata.
/local/
/objects/
/iterators/
/*.html
*.tmp
```

This file is a default Git hygiene policy, not a fence or guarantee.
`git add -f` can override it, and files already present in the Git index remain
tracked until the user explicitly removes them from the index. Its purpose is
to make ordinary `git add .covmap` stage the durable artifact without also
staging cache or checkout-local state.

The distinction is semantic:

| State | Default | Reason |
| ----- | ------- | ------ |
| `.gitignore`, `version`, `HEAD`, `ignore` | tracked | defines the project artifact and its default publication policy |
| `coverings/` | tracked | the covering is durable project knowledge |
| `connections/` | tracked | the mapping is durable project knowledge |
| `objects/` | ignored | local content cache reconstructible for every clean live segment |
| `local/peers` | ignored | contains machine-specific checkout paths |
| `iterators/` | ignored | cursor and task state are normally user/session-specific and high-churn |
| HTML and `*.tmp` | ignored | generated output and interrupted atomic writes |

### Object cache and hydration

A covering row already records the source path, exact line/column range, and
content hash. For an undrifted checkout, covmap can extract that range from the
source, recompute the content-only hash, and recreate the object. The tracked
ref and source are therefore the durable representation; `objects/` is an
operational cache.

```text
covmap hydrate [<covering>]
```

`hydrate` visits each selected covering entry:

1. read its source file and extract the recorded range;
2. recompute the content hash;
3. if the hash matches the ref, verify the existing object or write it when
   missing;
4. if the file/range is unavailable or the hash differs, report drift and do
   not write an object under the recorded hash.

The command is idempotent, never changes a ref, and exits non-zero if any live
entry cannot be reconstructed. `covmap fsck --offline` performs the same
source/hash verification without populating the cache by default;
`covmap fsck --offline --hydrate` verifies and fills every reconstructible
missing object in one pass. A missing cache object is not corruption when the
checked-out source reproduces its hash; an existing object whose bytes do not
hash to its filename is corruption.

A clean checkout with an empty `objects/` directory and source matching every
covering row must therefore pass `fsck --offline`. The current
`missing in object store` failure must not survive as a fresh-clone error; this
is the first regression gate for the cache-tolerance implementation.

Normal covering inspection opportunistically warms the cache. `status`,
covering/segment `show`, and `ls` write a missing object whenever the inspected
live source reproduces its hash. This is a cache-only side effect: it changes
neither refs nor project meaning and remains invisible to Git. If an inspected
entry has no object and its source has already drifted, the command emits a
one-line warning immediately:

```text
cache missing for main:@parse and source has drifted; remap needs recorded
bytes (restore a matching revision and run `covmap hydrate`)
```

This prevents the degraded state from remaining silent until a later remap.
Inspection-time warming is best-effort: an unwritable or read-only cache never
turns otherwise-successful inspection into failure and produces at most a
debug-level write notice. The drift warning above is independent and still
prints when the cache is unwritable. An object that already exists but
mis-hashes is different: it is reported as cache corruption.

Hydration and warming install objects atomically with a unique temporary file
in the target directory followed by create-if-absent publication. Concurrent
writers for one content hash therefore either observe the completed object or
discard an identical losing temporary file; no cache lock is required, and a
reader never observes a partial object.

Commands that need recorded bytes first try the cache. When the object is
missing but the live source still matches, they may hydrate it lazily. When the
source has already drifted and the old object is absent, operations that need
the old bytes, such as diff-remap or fingerprint relocation, fail with a clear
instruction to restore a matching source revision, run `covmap hydrate`, then
return to the edited revision. This is the trade-off for not publishing raw
source bytes twice, so the documented clone workflow hydrates before editing.

This rule applies to refinement as well. On an empty cache, `split` and `merge`
may hydrate each input only after the checked-out range reproduces its recorded
hash; they never substitute drifted working-tree bytes for recorded bytes.
CIP 0009's multi-cut transition remains atomic: resolve and validate every
input, write all fragment objects, then replace the covering ref once.

The resulting cases are explicit:

- **Authoring checkout:** `cover`, refinement, and `iter new` write objects as
  they create refs/tasks, so recorded bytes are already local.
- **Clean commit and clean clone:** source matches the refs; inspection,
  `hydrate`, or `fsck --offline --hydrate` restores the complete live cache
  losslessly.
- **Drifted committed state:** source and refs were committed out of sync;
  `fsck --offline` fails as the intended CI/publication gate. If no matching
  source revision exists, old bytes cannot be reconstructed.
- **Edit before first hydrate:** remap is temporarily degraded, but normally
  recoverable by restoring the clean committed revision, hydrating, and
  returning to the edit.

Bare-hash addressing must not depend only on directory entries in `objects/`.
Hash resolution searches the union of hashes in live covering refs and hashes
present in the local cache, deduplicating identical content hashes. A live hash
can therefore be resolved and lazily hydrated on a fresh clone. A hash that is
neither in a live ref nor in the local cache is intentionally unavailable; Git
tracking no longer makes unreachable historical objects globally addressable.
If a hash resolves through a live ref but the cache is absent and source has
drifted, content-fetch fails as **resolved but recorded bytes unavailable**,
not as `no such hash`; the diagnostic includes the covering/location and the
restore-and-hydrate recovery.

Iterators and objects have symmetric publication policy. `iter new` ensures
that every frozen task has a local object. With the default ignore file, both
the worklist and its frozen bytes remain local and retain the existing
append-only guarantee. A team that chooses to publish iterators must remove
both `/iterators/` and `/objects/` from `.covmap/.gitignore`, accepting the
repository-growth and raw-byte disclosure consequences. Tracking objects alone
is neither required nor recommended.

Because objects are ignored by default, `git add .covmap` does not publish raw
bytes from a Git-ignored source file. Covering refs still disclose covered path
names, hashes, labels, names, and messages; `.covmap/ignore` remains the control
for files that should not appear in the covering at all. A published covering
whose source files are not distributed with the project is incomplete:
`fsck --offline` reports those files as unavailable rather than treating the
hash-only ref as self-contained. A content hash is also a confirm-a-guess
oracle for short or low-entropy secret content, so secret paths belong in
`.covmap/ignore` even though their raw object bytes are not committed.

## Portable cross-repository connections

### Peer aliases

A **peer alias** is a source-repository-local name for another covmap
repository, such as `impl`, `spec`, or `firmware`. It is part of the mapping's
meaning and is committed in the connection ref. `.` is reserved for the current
repository.

Aliases match `[A-Za-z0-9][A-Za-z0-9._-]*`. The single character `.` is valid
only for the reserved self peer.

The connection format becomes:

```text
# source_covering=<C_A>
# target_peer=<alias-or-.>
# target_covering=<C_B>
<endpoint_A>	<endpoint_B>[	<note>]
...
```

The existing `# target_repo=<absolute-path>` field is removed. This is an
on-disk format change.

### Local bindings

An external peer alias is resolved through `.covmap/local/peers`:

```text
impl	/Users/alice/work/widget-impl
spec	/Users/alice/work/widget-spec
```

Relative binding paths are resolved from the current repository root.
`peer bind` discovers the target with `Repo::discover` and stores its canonical
covmap root, so symlinks and trailing separators do not create multiple
bindings for one checkout. Bindings are written atomically and sorted by alias.
They are local configuration, are never copied into connection refs, and are
ignored by the generated `.covmap/.gitignore`. Paths containing a tab or
newline are rejected because the binding format is line-oriented.

The reserved peer `.` always resolves to the current repository and must not
appear in the bindings file.

### Commands

```text
covmap peer bind <alias> <path>
covmap peer ls
covmap peer rename <old> <new>
covmap peer unbind <alias>
covmap connect <C_A> [<peer>:]<C_B> [<K>]
```

- `peer bind` discovers and version-checks the covmap repository at `<path>`,
  then records or replaces the local binding. A non-`.` alias may temporarily
  bind to the current repository for migration recovery, but `connect`
  canonicalizes that target to `.` and never stores a second durable spelling
  for self. `fsck` reports such a binding as noncanonical while refs still use
  the alias and points to `peer rename <alias> .`.
- `peer ls` prints the union of aliases referenced by local connection refs and
  aliases present only in the bindings file. It marks each as bound, unbound,
  missing, or invalid, so a fresh clone exposes every binding it needs.
- `peer rename` is crash-resumable rather than falsely claiming cross-file
  atomicity. It refuses `.` as `<old>` and an already-used external `<new>`.
  Renaming to `.` is permitted only when the old binding resolves to the
  current repository; this is the explicit recovery for a legacy self path
  that migration could not recognize. The command records `old -> new` in
  `.covmap/local/peer-rename`, retains the old binding, then rewrites
  connections one at a time in sorted filename order using each file's atomic
  write. For an external `<new>` it first installs the new binding; for `.`
  there is no binding. After every ref is rewritten it removes the old binding
  and journal. Re-running resumes the operation; `fsck` reports an unfinished
  local journal.
- `peer unbind` removes local resolution only; it never edits a connection.
- `connect` preserves CIP 0007's bare-covering self-map form: omitting
  `<peer>:` means `.`, so `covmap connect main main duplication` records
  `target_peer=.`. Explicit `.:<C_B>` is accepted as the equivalent spelling.
  An external form resolves `<peer>` before creation and verifies `<C_B>`.

Read-only commands degrade without corrupting their established interfaces.
`status` and connection `show` render an unbound target as
`unavailable (peer 'impl' is not bound)`. Raw `ls-links` needs no binding and
prints the stored endpoints. `ls-links --resolve` preserves its TAB-delimited
stdout schema, leaves unavailable target label/location fields empty, and
prints one actionable warning to stderr. This reuses the existing unresolved
endpoint convention: both target fields empty means unresolved, while an empty
target label with a non-empty target location means a resolved, unlabeled
segment. `fsck --offline` reports the external side as unchecked. `disconnect`
reads and removes the local connection ref without resolving its peer, so the
full CIP 0007 lifecycle remains available in a clean clone.

Commands that require target content (`link`, target-side resolution, `prune`,
HTML compare rendering, and full `fsck`) fail with the actionable instruction
`peer 'impl' is not bound; run covmap peer bind impl <path>`. In particular,
`prune` must never interpret an unavailable target as a set of broken links.

This keeps the committed mapping stable when two developers use, for example,
`~/work/impl` and `/src/impl`. The alias is shared; the paths are not.

The self peer never compares path strings during normal operation: `.` resolves
directly to the already-discovered current `Repo`. Only format-2 migration must
recognize an old self path, and it does so by canonicalizing both roots (using
filesystem identity where the platform exposes it), not by comparing their
stored spelling.

## Deterministic refs and Git behavior

Every write shall produce a canonical byte representation:

- covering entries are ordered by file, start position, end position, then
  hash;
- connection links are ordered by source endpoint, target endpoint, then note;
- peer bindings are ordered by alias;
- metadata headers have a fixed order and files use LF endings;
- rewriting unchanged logical state produces no byte change.

Canonical ordering reduces review noise and makes independent additions more
likely to merge cleanly. Segment message indexes are computed after covering
entries are put in canonical order.

`ls-links` and `ls-links --resolve` retain their existing TAB-delimited record
schemas and emit rows in the connection ref's canonical order. The format-2 to
format-3 migration may therefore reorder link rows once. This changes no
relation semantics because a connection is a set; scripts may rely on the
record schema, but not on historical insertion order. Diagnostics remain on
stderr rather than contaminating plumbing stdout.

After a checkout, rebase, or merge:

```sh
covmap fsck --offline
```

validates the version, covering partitions, source content against each
recorded hash, any cached objects that are present, connection syntax, source
endpoints, local binding syntax, unfinished local journals, and all
self-connections without requiring external peer checkouts. It reports
external target endpoints as unchecked, not valid. Plain `covmap fsck` keeps
the stronger existing contract: every peer must be bound and every endpoint
must resolve. Neither mode requires a cache object when the source reconstructs
it exactly.

Verification diagnostics distinguish:

- **source drift** — the ref is structurally valid, but current source no
  longer hashes to its recorded value; repair the covering after the
  in-progress edit;
- **cache corruption** — an object exists but its bytes do not hash to its
  filename; replace it by deleting the cache entry and hydrating clean source.

Both make `fsck` non-zero, but only cache corruption means covmap's stored bytes
are internally inconsistent. Source drift during active editing is expected
workflow state, not described as corruption.

Conflict markers or a ref/object mismatch are ordinary integrity failures;
covmap must not guess how to repair a semantic merge conflict.

## Workflow

### Initial publication

```sh
covmap init
printf 'build/\n.env\n' > .covmap/ignore
covmap cover main
covmap fsck --offline
git add .covmap
git commit -m "Add project covering"
```

`init` does not inspect Git state or stage files. It prints the publication
boundary explicitly:

```text
Project artifacts: .covmap/coverings, .covmap/connections, and covmap metadata
Local state:       .covmap/objects, .covmap/iterators, .covmap/local
Next:              git add .covmap
```

The objects written by `cover` remain local because the generated default
ignore policy excludes them.

### Portable cross-repository mapping

```sh
# In the spec checkout:
covmap peer bind impl ../widget-impl
covmap connect main impl:main spec_to_impl
covmap link spec_to_impl @parse @parse
covmap fsck
git add .covmap
git commit -m "Map parser spec to implementation"
```

After cloning the spec project elsewhere:

```sh
covmap fsck --offline --hydrate   # validate and warm cache before editing
covmap peer bind impl ~/src/widget-impl
covmap fsck                       # validates both coverings and every link
```

Only ignored cache/local state differs between those machines.

Teams that want automatic warming may install a Git `post-checkout` and/or
`post-merge` hook that runs `covmap fsck --offline --hydrate`. Documentation
provides the hook body, but covmap never installs it or changes `.git/`; this
keeps the integration opt-in and preserves Git independence.

### Branch and review behavior

A source edit and its covering repair are committed together. Reviewers see
the source diff plus the changed covering/connection rows, without a duplicate
set of hash-named content files. If branches refine different regions, Git
performs a normal text merge on the refs; `fsck` is the semantic gate
afterward.

## Invariants

- **Reconstructible live state.** In a clean checkout, extracting every
  covering row's source range yields bytes whose content hash equals the row's
  recorded hash. Those bytes can recreate every live cache object.
- **Cache independence.** Missing cache objects do not invalidate otherwise
  reconstructible refs; present cache objects must hash to their filenames.
- **Post-clone remap safety.** Full diff-remap/fingerprint guarantees on a
  fresh clone require hydration before the first covered-source edit.
  Inspection warms reconstructible entries opportunistically, offline fsck
  offers `--hydrate`, and missing-object drift is warned immediately.
- **No machine path in a durable ref.** Coverings and connections are
  byte-stable across checkout locations.
- **Explicit peer resolution.** An external target is accessed only through a
  local alias binding; `.` is the only implicit peer.
- **Deterministic serialization.** Equal logical state serializes to equal
  bytes.
- **Strong verification remains available.** Full `fsck` never treats an
  unbound or unavailable target as resolved; `--offline` clearly distinguishes
  unchecked external endpoints.
- **Git independence.** All semantics are defined in terms of files and paths;
  covmap neither requires nor mutates a Git repository.
- **Local frozen content.** Default-local iterators retain their content
  through the default-local append-only cache; publishing either requires an
  explicit policy change for both.

## Migration (format 2 to 3)

`covmap migrate` retains the existing format-1 to format-2 migration and adds a
resumable format-2 to format-3 phase. A format-1 repository runs the phases in
sequence and reaches format 3 in one invocation.

On entry, an existing valid manifest resumes publication at step 4. Otherwise,
the format-2 to format-3 phase is:

1. If `.covmap/.gitignore` is absent, atomically create the standard policy
   before creating `.covmap/local/migrate-2-to-3/` and its manifest. If the
   ignore file already exists, preserve it byte-for-byte and report any
   standard local-state exclusions it lacks; migration never overwrites
   user-managed ignore policy.
2. Read all format-2 connections and coverings before replacing any live ref.
   For reachable legacy targets, use unchecked repository discovery so a
   format-2 peer need not already be migrated. Compare the discovered target
   with the current repository only to recognize self. Use each exact stored
   legacy target string as the deterministic external key and sort those keys
   lexicographically, then:
   - if a reachable target is the canonical current repository, assign
     `target_peer=.`;
   - otherwise derive an alias from the stored path's sanitized directory
     basename, assigning `-2`, `-3`, ... in sorted-key order on collisions;
   - stage the old target path as that alias's initial local binding;
   - replace `target_repo` with `target_peer` and canonically sort links.
3. Stage the canonical format-3 connection and covering bytes and bindings
   before publishing any replacement.
4. Atomically publish each staged file and record its completion in the
   manifest. While `.covmap/version` remains `2`, a repeated `migrate` resumes
   from this manifest instead of trying to parse already-published format-3
   refs as format 2.
5. Set `.covmap/version` to `3` only after every replacement is complete, then
   remove the migration staging directory and manifest.

Migration prints every generated `alias -> path` binding so the user can run
`covmap peer rename <old> <new>` before committing. It is idempotent. It does
not add anything to Git and does not delete existing objects; they become
ignored local cache entries under the standard policy. Without inspecting Git,
migration always notes that ignore rules do not untrack indexed files and shows
the explicit `git rm --cached` recovery for projects that need it.

An unavailable old absolute target does not prevent migration: its basename can
still seed the alias and its path can still become the initial binding. Because
an unavailable path cannot safely be inferred to mean self, migration warns
about that ambiguity. The user can confirm it explicitly with
`covmap peer bind <alias> .` followed by `covmap peer rename <alias> .`.
Otherwise full `fsck` reports the peer unavailable until its binding is
repaired. External peer repositories may migrate independently; binding and
full verification require them to support format 3.

The one-time canonical rewrite in steps 2-4 is deliberate. Format 3 promises
that equal logical refs have equal bytes, so migration establishes the invariant
immediately rather than leaving old refs order-dependent until their next
mutation.

## Alternatives considered

### Track the object store

Tracking the full append-only store gives every clone historical segment bytes,
but its working tree grows monotonically and Git history makes the growth
irreversible. It also creates many small files and can publish raw bytes from
source files Git otherwise ignores. Tracking only objects reachable from live
coverings bounds the current tree, but still duplicates the checked-out source
and requires a synchronization/garbage-collection protocol.

The default therefore tracks refs and reconstructs live objects with
`hydrate`. Iterator snapshots are the genuine non-reconstructible case after
source drift; they remain local together with their objects. A future snapshot
or export proposal may define a compact, explicit publication format for
frozen content without making the operational cache part of every project
commit.

### Keep absolute paths or store relative paths

Absolute paths are machine-specific. Relative paths work only when every clone
uses the same workspace topology. Peer aliases preserve project meaning while
allowing each checkout to bind that meaning to its own layout.

### Use Git remote URLs as repository identity

The same repository may be cloned through SSH, HTTPS, mirrors, or no remote at
all. Requiring Git metadata would also violate Git independence. A local alias
is sufficient for connection semantics and deliberately leaves repository
discovery to the user/project.

### Automatically stage or commit covmap state

This hides an important publication boundary. Explicit `git add .covmap` keeps
normal review and policy controls in charge, while the nested ignore file
prevents operational cache bytes from being staged by default.

## Landing checklist (six-artifact consistency)

Per `DEVELOPER.md`, this is a new capability, a connection behavior change, and
an on-disk format change:

- **Theory** (`theory/main.tex`): distinguish durable serialized artifacts from
  local resolution state; define a connection's repository endpoint as a
  logical peer rather than a filesystem path. The mathematical relation
  `κ ⊆ Seg(Cov^A) × Seg(Cov^B)` is unchanged. Reconcile the iterator snapshot
  guarantee with its default-local cache, and state that refinement may
  reconstruct a missing recorded object only from source that reproduces its
  hash.
- **Requirements** (`REQUIREMENTS.md`): realize req24, which owns the
  Git-trackable covering/mapping capability and composes portable deterministic
  refs, source-verifiable hydration, the durable/local boundary, peer aliases,
  default publication policy, and Git independence. Amend req15 for full vs
  offline verification and req17 for the durable-ref/local-cache boundary and
  format 3. Reconcile req21 so iterator durability is explicitly local unless
  a team opts in to publishing both iterators and objects. State the post-clone
  remap-safety condition and require diagnostics to distinguish source drift
  from cache corruption. Amend req19/req20 to preserve `ls-links` field schemas
  while defining canonical relation order, peer-alias presentation, and
  unbound-peer behavior.
- **Implementation** (`src/`): add peer binding read/write/resolution and the
  `peer` command group; replace `Connection.target_repo` with `target_peer`;
  preserve bare-covering self-map syntax and unbound `disconnect`; route every
  target lookup through one resolver; make `peer ls` include aliases named only
  by refs; generate `.covmap/.gitignore` in `init` with explanatory comments and
  objects/iterators/local/report/temp exclusions; print the exact
  project-artifact/local-state boundary and `git add .covmap` next step; add
  `hydrate`; make drift/status/fsck verify clean live content directly when an
  object is absent; opportunistically warm missing objects during
  `status`/`show`/`ls` on a best-effort basis; install cache objects atomically
  with race-safe create-if-absent semantics; add `fsck --offline --hydrate`;
  resolve bare hashes over live refs plus cached objects; distinguish `no such
  hash` from `resolved but recorded bytes unavailable`; distinguish source
  drift from cache corruption; retain clear failures for drift repair that
  needs missing historical bytes; make peer rename journaled and resumable;
  preserve recorded-byte semantics for `split`/`merge`; canonicalize
  covering/connection writes and plumbing row order; chain the existing
  format-1 migration through a resumable format-2 to format-3 migration; update
  the layout/version comment in `src/repo.rs` for format 3. Test
  clone-location independence, hydrate success/failure/idempotence, inspection
  warming, read-only/unwritable-cache inspection success, concurrent warming
  of one hash without partial reads or locking, offline-fsck hydration, warning
  on edit-before-hydrate, drift-vs-corruption output and exit status, lazy
  cache fill and bare-hash resolution on an empty cache,
  resolved-but-unhydratable content fetch, unavailable unpublished source,
  ignored local state, iterator/object policy symmetry, canonical self peers,
  interrupted peer rename recovery including migration-to-self, missing
  bindings, unbound disconnect, canonical rewrites and `ls-links` schemas,
  multi-cut/merge with an empty cache and with drift, offline/full fsck,
  interrupted migration resumption, format-1 chaining, deterministic migration
  alias collisions, migration preservation of a custom nested ignore file, and
  nested atomic `*.tmp` ignores including object paths. Test the exact generated
  ignore file and `init` guidance; document that forced adds bypass ignores and
  that already-tracked objects/iterators require an explicit index removal.
- **Skill** (`SKILL.md` + `skill_playground/`): teach committing `.covmap/`,
  hydrating immediately after clone, optional post-checkout/post-merge hook
  integration, binding peers, canonical self-map syntax, and running
  offline/full integrity checks. Add T12 as a two-checkout task proving the
  same committed connection works with different absolute paths and without
  committed objects. Show the expected staged paths and the
  `git rm --cached` recovery for previously tracked local state.
- **Tutorial** (`tutorial/TUTORIAL.md` + `tutorial/run_tutorial.sh`): commit or
  copy a Git-ready `.covmap/`, clone it to a different path, run
  `fsck --offline --hydrate`, bind its peer, and verify the mapping without
  editing the committed connection. Demonstrate the edit-before-hydrate
  warning/recovery once. Assert that objects, iterators, reports, bindings, and
  atomic temp files remain ignored, and show the paths staged by
  `git add .covmap`.
- **`GLOSSARY.md`**: add *artifact*, *peer alias*, *peer binding*, *offline
  verification*, *hydration*, *object cache*, and the durable-ref/local-cache
  distinction. Amend *iterator*, *frozen segment*, and *object store* so their
  retrieval guarantees match the default-local publication policy.
- **`README.md`**: document the tracked ref set, ignored object cache,
  post-clone hydrate/bind workflow, optional Git hook, drift-vs-corruption
  diagnostics, ignore-policy override/already-tracked caveats, expected staged
  paths, missing-historical-byte limitation, format 3 layout, and migration.

**`FORMAT_VERSION` 2 -> 3.** The connection ref no longer stores
`target_repo=<absolute-path>`; all target access resolves a `target_peer`
through local bindings.
