# Positive covering-input selection

Stage: **Landed**

## Relationship to CIP 0010

[CIP 0010](0010-git-tracked-artifacts.md) landed before this proposal and is
the baseline for it. Format 3 makes coverings, connections, and project
configuration portable Git-trackable artifacts, while caches, peer bindings,
and iterator work state remain local.

CIP 0010 already classifies `.covmap/ignore` as tracked covering-input policy.
This proposal extends that policy in two directions:

1. add positive include patterns; and
2. allow a covering to override the positive default and add its own
   exclusions.

All new policy files are durable project artifacts. They are not ignored by
the generated `.covmap/.gitignore`, contain no checkout-specific paths, and
must behave identically in every clone.

Connections and mappings do not receive a separate selection policy. A
connection relates two already-materialized coverings; each endpoint
covering's repository controls how future files enter that covering.

## Requirement ownership

This CIP is the design vehicle for extending req18 from ignore-only control to
positive and covering-specific selection.

Supporting requirements retain narrower ownership:

- req7 owns additive, idempotent `cover`;
- req17 owns the versioned on-disk substrate and format-4 compatibility gate;
- req24 owns portability and the project-artifact/local-state boundary; and
- req6 continues to own the actual covered set declared by a covering ref.

Selection policy changes which new files `cover` considers. It does not change
req6 semantics or connection relation semantics.

The covering-name validator below is a prerequisite security correction to
the existing req17/req24 artifact implementation, not new req18 selection
semantics. It must be reviewable and testable independently of the feature.

## Idea

Keep the existing repository-wide `.covmap/ignore`, add an optional
repository-wide `.covmap/include`, and add tracked covering-specific policy
files:

```text
.covmap/
  include                         optional global positive default
  ignore                          optional global exclusion policy
  selections/<C>/include          optional positive override for covering C
  selections/<C>/ignore           optional additional exclusions for C
```

For covering `C`, a covering-specific include file replaces the global
include. If neither exists, inclusion defaults to every discoverable file.
Global ignore always applies, and a covering-specific ignore adds more
exclusions:

```text
include(C) = specific include, else global include, else all
ignore(C)  = global ignore union specific ignore
selected(C) = discoverable AND include(C) AND NOT ignore(C)
```

`covmap cover [<C>]` uses that resolved policy only when discovering new
files. It remains additive and never removes an existing covering entry.
The covering ref remains the authoritative current covered set `F(C)`.

## Motivation

Negative selection works well when most repository files belong in every
covering and only a small stable set does not. It is cumbersome when the
desired set is small or when one repository maintains several coverings with
different responsibilities.

Examples include:

- one covering for requirements and another for formal theory in the same
  repository;
- a documentation covering that should admit `/docs/`, while an
  implementation covering should admit `/src/`;
- a focused mapping that names a few exact source-of-truth files among
  generated or vendored material; and
- a project that wants future files admitted only under explicitly selected
  directories.

A single repository-wide include cannot express those first two cases: taking
the union causes every covering to admit both trees, while taking the
intersection admits neither. Covering-specific positive overrides provide the
needed scope without inventing policy on connections.

The distinction between policy and state remains essential:

- selection policy controls candidates considered by future `cover` runs;
- `cover` is additive and preserves every existing refinement and annotation;
  and
- covering refs record the exact current covered set through their entries and
  derived `# covered=` lines.

The policy files are therefore not synchronizing manifests.

## Goals

1. Make small positive file sets concise and reviewable.
2. Support different discovery scopes for several coverings in one
   repository.
3. Preserve current ignore-only behavior when no new policy file exists.
4. Make policy portable as part of CIP 0010's Git-trackable project artifact.
5. Give global and covering-specific policy deterministic, order-independent
   composition.
6. Preserve additive, idempotent `cover` behavior and all covering metadata.
7. Prevent format-3 binaries from silently ignoring the new tracked policy.

## Non-goals

- Removing files from an existing covering.
- Synchronizing a covering exactly to current selection policy.
- Adding selection policy to a connection or mapping.
- Allowing a covering-specific rule to override a global ignore veto.
- Full `.gitignore` syntax, including negation, `**`, character classes, or
  ordered last-match-wins rules.
- Following symbolic links or changing UTF-8 and empty-file eligibility.
- Including dot-prefixed paths.
- Invoking Git, inspecting the Git index, or staging policy files.

## Prerequisite: covering-name path and address safety

The current format-3 implementation joins an unvalidated covering name onto
`.covmap/coverings`. A command such as `covmap cover ../../../PWNED` can
therefore write outside the repository. This is a pre-existing path traversal
bug, not a consequence of positive selection, and must land as an independently
reviewable security fix before the selection loader or format-4 layout.

Covering names also participate in the `<C>:<address>`,
`[<peer>:]<covering>`, and `@name` command grammars and are stored in
line-oriented refs. A path-safe name must therefore also round-trip without
being consumed as an address delimiter, sigil, comment leader, or whitespace.

For every supported format, define `valid_covering_name(n)` exactly as:

```text
n is valid UTF-8 and non-empty
n is not exactly ".", "..", or "HEAD"
n contains none of "/", "\", ":", NUL, or Unicode whitespace
n does not begin with "@" or "#"
```

No normalization or separator replacement occurs at ordinary command entry.
A name either satisfies this grammar byte-for-byte or is rejected.

The validator must be enforced before side effects at every covering-name
entry point, not only `cover`: the optional or `HEAD`-resolved name accepted by
covering commands; the central covering-ref read/write/path helper; covering
enumeration; `.covmap/HEAD`; connection source and target covering fields;
iterator covering metadata; selection paths; migration; and full or offline
`fsck`. Callers must not construct a covering or selection path with a direct
`join(name)`.

The bug-fix change itself does not depend on include parsing, selection
precedence, or format 4 and does not require a format bump. It may ship in the
same release as format 4 so repositories containing a legacy unsafe name are
not stranded, but it remains a separate implementation and test unit.
Write-side regression tests must prove that rejected names create or modify no
path inside or outside the repository. `covmap migrate` is the sole exception
that may parse an unsafe legacy name, and only through the bounded,
non-symlink recovery and staged rewrite described below.

## Project-artifact layout

Format 4 extends CIP 0010's project artifact:

```text
<repo>/.covmap/
  .gitignore                     tracked default publication policy
  version                        tracked format version
  HEAD                           tracked default covering
  include                        tracked optional global positive default
  ignore                         tracked optional global exclusions
  selections/<C>/include         tracked optional positive override
  selections/<C>/ignore          tracked optional additional exclusions
  coverings/<C>                  tracked covering refs
  connections/<K>                tracked connection refs
  objects/                       ignored reconstructible cache
  local/                         ignored checkout-local state
  iterators/                     ignored work-session state
```

The generated `.covmap/.gitignore` needs no exclusion for selection policy:
`include`, `ignore`, and `selections/` are intentionally published by ordinary
`git add .covmap`. Its broad legacy `*.tmp` rule is narrowed to the literal
escaped atomic namespace `\#covmap-tmp-*.tmp`, so valid project refs such as
`coverings/release.tmp` remain tracked. Its explanatory comment and
`covmap init` output should name selection policy explicitly as project
metadata.

`selections/<C>/` uses the same covering name as
`.covmap/coverings/<C>`. Policy may be written before the first
`covmap cover <C>`; this is how a new covering receives a deliberate scope
before it has a ref. A selection directory without a covering ref is
therefore not an integrity error.

The literal `HEAD` is never a policy directory name. `cover` resolves `HEAD`
to its actual covering name before loading policy.

`<C>` must satisfy `valid_covering_name`. This makes both artifact paths direct
children of their declared directories and prevents ref or policy lookup from
escaping `.covmap`. The common validator owns the path, delimiter, whitespace,
and sigil character grammar; individual commands must not re-interpret one of
its valid names as a peer prefix, segment address, or ref comment.

Policy files remain user-authored plain text. Pattern order has no selection
semantics; source order and line numbers are retained only for stable,
actionable diagnostics.

## Pattern grammar

Every include and ignore file uses the existing gitignore-lite line grammar:

- blank lines and lines beginning with `#` are skipped;
- `*` and `?` match within one path component and do not cross `/`;
- a leading `/` anchors a pattern at the repository root;
- an unanchored pattern may match a basename or trailing path fragment; and
- a trailing `/` is the conventional directory/subtree spelling.

Examples:

```text
# Exact, root-relative files
/README.md
/src/parser.rs

# A whole subtree
/spec/

# Matching basenames at any depth
*.proto
```

No negation or rule ordering is added. In particular, `!` does not become a
re-inclusion operator. Include files are positive unions; ignore files are
negative unions.

Root-anchored exact paths are the recommended spelling for an explicit file
manifest.

### Entry and file-policy matching

The current walker applies patterns to individual filesystem entries. Retain
that operation as `entry_match(p, x)`, where `x` is one repository-relative
file or directory path.

Positive selection and policy diagnostics ultimately decide whether a file is
admitted. A directory pattern must therefore apply to every descendant. For a
file `f`, let `ancestors(f)` be its repository-relative parent directories,
from the immediate parent to the top-level parent:

```text
file_match(p, f) =
  entry_match(p, f)
  OR any d in ancestors(f) satisfies entry_match(p, d)
```

This is also the exact file-level interpretation of current ignore traversal:
the live walker excludes a file when the file entry matches or when it pruned
a matching ancestor directory.

A trailing slash is stripped by `entry_match` as it is today. Consequently
`src` and `src/` remain match-equivalent, including for a same-named regular
file. The slash communicates directory intent but is not a kind constraint.
This legacy edge case is retained so existing `.covmap/ignore` files do not
change meaning.

Examples:

```text
file src/main.rs         matches /src/
file src/deep/main.rs    matches /src/
file other/src/main.rs   does not match /src/
file other/src/main.rs   matches src/
file docs/reference/x.md matches /docs/reference/
file src/main.rs         matches /src/*.rs
```

`file_match` is the common predicate for include admission, off-walk ignore
evaluation, per-pattern diagnostics, and already-covered policy-drift
diagnostics.

## Effective policy

Let:

- `G+` be patterns from `.covmap/include`, or absent;
- `G-` be patterns from `.covmap/ignore`, defaulting to the empty set;
- `C+` be patterns from `.covmap/selections/<C>/include`, or absent; and
- `C-` be patterns from `.covmap/selections/<C>/ignore`, defaulting to the
  empty set.

Define the effective positive pattern source:

```text
I(C) =
  C+       when the covering-specific include file exists
  G+       otherwise, when the global include file exists
  ALL      otherwise
```

The distinction between absent and empty is significant. A present empty or
comment-only covering-specific include selects no new files; it does not fall
back to the global include. The same rule applies to a present empty global
include when no specific include exists.

For a repository path `f`:

```text
included(C, f) =
  true                       when I(C) = ALL
  any p in I(C): file_match(p, f) otherwise

ignored(C, f) =
  any p in G- union C-: file_match(p, f)

admitted(C, f) = included(C, f) AND NOT ignored(C, f)
selected(C, f) = f in D(A) AND admitted(C, f)
```

`D(A)` is the current discoverable set: regular files reached without
following symbolic links, excluding dot-prefixed entries.

Consequences:

1. With only the existing `.covmap/ignore`, behavior is unchanged.
2. Global include is a default for coverings without a specific include.
3. A specific include replaces, rather than unions with, global include.
4. Global ignore is a hard veto for every covering.
5. A specific ignore adds exclusions for one covering.
6. Pattern and file order never change the selected set.
7. Missing include and empty include have deliberately different meanings.

The global-ignore veto is intentional. It remains the repository-wide place
for secrets, generated output, and paths that should not appear in any
published covering. A future need to override it for one covering should be
designed explicitly rather than added through negation or ordering.

## Directory traversal

Include patterns filter files, not traversal. The walker must descend through
a non-ignored directory that does not itself match an include pattern because
a deeper file may match, as with `/src/*.rs`.

Global and covering-specific ignore patterns may prune a matching directory
immediately through `entry_match`. The ancestor branch of `file_match` makes
read-only policy evaluation agree with the live walker's exclusion.

An implementation may optimize positive traversal only when it can prove that
no effective include pattern can match a descendant. Direct failure of the
directory entry to match is not enough.

Dot-prefixed entries, including `.covmap` and `.git`, remain unconditionally
undiscoverable.

## `cover` behavior

The command surface remains:

```text
covmap cover [<covering>]
```

The command resolves its covering name first, loads the effective policy for
that name, and adds one whole-file `unknown` segment for each selected
readable, non-empty UTF-8 file not already covered.

Selected binary files retain the existing warning and are skipped. Empty files
remain unrepresentable by the current non-empty segment model.

The operation remains additive:

- narrowing any include policy does not remove existing covered files;
- widening any ignore policy does not remove existing covered files;
- widening an effective include makes newly selected files eligible on the
  next run; and
- changing policy never rewrites refinements, labels, names, messages, links,
  peers, or iterators.

### Policy-drift warning

When covering `C` already contains a path for which `admitted(C, f)` is false,
`cover C` prints a non-fatal warning that the path was left unchanged. The
comparison is path-based and independent of whether the file currently
exists, so missing-source drift is not reported a second time as policy drift.
Paths are listed in stable order and a long list may be bounded with an
omitted count.

The warning is intentionally sticky. It appears on every later `cover C` run
until the path is admitted again or a future explicit file-removal operation
removes it. It exits successfully and does not block additions. No
acknowledgement state is added because that would create another policy source
without solving removal.

### Unmatched include warnings

For each active pattern line in the effective include source `I(C)`, `cover C`
warns when that pattern matches no discoverable file under `file_match`, even
if other patterns match. The warning identifies the policy file, source line,
and pattern.

Matching is counted before ignore vetoes. A valid include pattern whose files
are deliberately excluded by global or covering-specific ignore is not itself
reported unmatched.

Only the effective include source is checked. When a specific include
overrides global include, inactive global patterns do not warn during
`cover C`; they may still be active for other coverings.

An empty effective include is valid, selects no new files, and emits no
unmatched-pattern warnings.

## Mappings and peers

Selection policy applies to `cover`, not to connections:

- a same-repository connection may relate coverings with different effective
  policies;
- an external connection's source covering uses policy from the source
  repository, while its target covering was built under policy committed in
  the peer repository;
- binding or unbinding a CIP 0010 peer never changes either covering's
  selection policy; and
- changing policy never adds, removes, or rewrites a stored link.

Connection coverage and endpoint resolution continue to use the actual
covering refs. They never infer membership from include or ignore files.

## Examples

### Global positive default

```text
# .covmap/include
/docs/
/src/
```

Every covering without a specific include uses these two subtrees. A newly
added unrelated directory is not admitted.

### Different scopes in one repository

```text
# .covmap/ignore
/target/
*.generated
```

```text
# .covmap/selections/spec/include
/requirements/
```

```text
# .covmap/selections/implementation/include
/src/
/tests/
```

Then:

```sh
covmap cover spec
covmap cover implementation
covmap connect spec implementation spec_to_impl
```

The `spec` covering admits only `requirements/`; the `implementation`
covering admits `src/` and `tests/`. Both exclude the global ignored paths.
The connection relates those resulting coverings and needs no selection
configuration of its own.

### Specific include overrides global include

```text
# .covmap/include
/docs/
```

```text
# .covmap/selections/theory/include
/theory/
```

`cover main` uses `/docs/` when `main` has no specific include. `cover theory`
uses only `/theory/`; it does not take the union with `/docs/`.

### Additional covering-specific exclusion

```text
# .covmap/selections/implementation/ignore
/src/generated/
```

This exclusion applies only to `implementation`, in addition to every global
ignore pattern.

## Invariants

1. Policy files are portable project artifacts and contain no checkout-local
   path.
2. Equal repository contents and policy files produce the same selected set in
   every clone.
3. Policy resolution is keyed by the resolved covering name, never the literal
   `HEAD`.
4. Global ignore is always a veto.
5. A covering-specific include replaces global include for that covering.
6. A `cover` run never removes or rewrites an existing covering entry.
7. Re-running `cover C` without repository or effective-policy changes adds
   nothing.
8. The covering ref, not selection policy, is authoritative for `F(C)`.
9. Connections depend only on actual endpoint coverings, not their policies.
10. Existing format-3 ignore-only repositories retain their selected set after
    migration.
11. No selected path escapes the repository walk or enters through a symbolic
    link.
12. Every covering ref and covering-specific policy is keyed by the same
    validated path- and address-safe single-component name.

## Compatibility and format 4

Format 3 binaries do not know `.covmap/include` or
`.covmap/selections/`. If they opened a repository using these files,
`covmap cover` could add paths outside the committed positive policy.
Additivity does not make that safe because covmap has no supported file
removal operation and the unwanted entries may later be refined or linked.

This proposal therefore bumps the repository format from 3 to 4. The
covering, connection, peer, iterator, and cache encodings do not change.

Format-3 to format-4 migration is metadata-only:

1. inventory covering names from legacy covering paths, `HEAD`, connection
   source and target fields, iterator metadata, and any pre-authored selection
   directories before replacing a live artifact;
2. map every legacy name through the deterministic escape function below;
3. stage covering refs under their mapped direct-child names, rewrite `HEAD`,
   both covering fields in every connection, iterator covering metadata, and
   selection-directory names with the same map, while preserving ref, task,
   and policy contents;
4. validate every mapped artifact name and every present policy before
   activating format 4;
5. create no include or covering-specific policy when it is absent;
6. create `selections/` lazily or as an empty directory;
7. publish all staged project artifacts and rewritten local iterator metadata,
   then publish version `4` last; and
8. print every legacy-to-format-4 covering-name rewrite and report that
   existing ignore-only discovery behavior is unchanged.

Let `P` be the ASCII prefix `covmap-escaped-`. For the UTF-8 byte string of a
legacy name `n`, using the full 256-bit SHA-256 digest rather than covmap's
truncated segment hash:

```text
migrate_name(n) =
  n                                      when valid_covering_name(n)
                                           and n does not start with P
  P + lowercase_hex(sha256(utf8(n)))     otherwise
```

The prefix rule is part of the migration algorithm, not an additional
format-4 name restriction. Unchanged names never begin with `P`, while every
rewritten name does. A valid legacy name that already begins with `P` is
therefore escaped too, preventing a pre-existing name from occupying the
generated namespace. The fixed 79-byte result remains below common filesystem
component limits even when the legacy spelling was long. Migration verifies
that distinct inventoried names have distinct outputs before writing; a
collision is a hard pre-publication failure. Independent repositories rewrite
the same external endpoint covering name identically, even when a peer is
unavailable during migration.

Migration-specific legacy lookup must not feed an unsafe name back through an
ordinary path join. It inventories directory entries and, for a referenced
legacy spelling such as `../evil`, may recover the old ref only after lexical
normalization plus non-following metadata checks prove that the source is a
non-symlink regular file beneath the canonical `.covmap/` directory. It then
stages that ref as `.covmap/coverings/<migrate_name(n)>`. A legacy spelling
that resolves outside `.covmap/`, an invalid-UTF-8 filename, a symlink, an
unexpected nested layout that cannot be assigned one identity, or two
different source artifacts claiming the same identity is refused before any
live write, with the exact offending path and manual recovery guidance.
Migration never scans or mutates paths outside `.covmap`; files written there
by an older traversal bug are not covmap artifacts and cannot be inferred
safely.

An escaped ref created by an explicit legacy command need not be named by
`HEAD` or any local connection, so its original spelling may be absent from
all inventory metadata. Migration therefore accepts a narrow repeatable
recovery option:

```text
covmap migrate [--recover-covering <legacy-name>]...
```

Each supplied legacy spelling is added to the inventory and mapped by
`migrate_name`; the user does not choose an arbitrary replacement. The option
is accepted only by migration, may name an otherwise-invalid legacy covering,
and is recorded in the ready manifest so a resumed migration does not require
the arguments again. Thus a format-3 repository produced by
`covmap cover ../evil` can be repaired with:

```sh
covmap migrate --recover-covering ../evil
```

provided the resolved old ref is the non-symlink regular file
`.covmap/evil`. Supplying a nonexistent name, a path outside `.covmap`, a
reserved metadata file that does not parse as that covering, or two spellings
for one source is a pre-publication error. This is a migration repair surface,
not a general covering rename operation.

For a format-1 or format-2 repository, the same inventory and name rewrite is
a preliminary resumable migration stage while the old version remains
published. It stages all affected names and stored references, records a
ready manifest, then publishes the safe namespace in deterministic order.
Re-running `migrate` resumes that stage before CIP 0010's existing migrations
continue to format 3; the format-4 metadata phase then follows. This prevents
an older phase from reading or rewriting an unsafe path through ordinary ref
helpers.

Version-mismatch diagnostics for formats 1, 2, and 3 must direct the user to
`covmap migrate`. The existing hard-coded wording `upgrade it to format 3`
must be replaced by a diagnostic derived from the binary's supported
`FORMAT_VERSION`, so it cannot go stale on this or a later bump.

The generated `.covmap/.gitignore` replaces the broad legacy `*.tmp` rule with
the literal escaped pattern `\#covmap-tmp-*.tmp`. Exact legacy generated
policies are upgraded during migration and by a repeat format-4 `migrate`;
custom policies are preserved with a warning when they retain the broad rule
or omit the reserved atomic namespace. Documentation and the generated comment
identify selection policies as tracked project metadata. An older binary sees
version 4 and rejects the repository before it can ignore the policy.

## Why not negation in `.covmap/ignore`

A construction such as:

```text
*
!src/
!src/*.rs
```

would turn the current unordered matcher into an ordered rule language.
Re-including a descendant also requires special treatment of ignored parent
directories. Separate positive and negative files make the set algebra
explicit and preserve current ignore behavior.

## Alternatives considered

### Repository-wide include only

This was the first draft. It is insufficient when one repository carries
several coverings with different responsibilities: the union over-admits and
the intersection under-admits. A global include remains useful as a default,
but a covering-specific positive override is required.

### Covering-specific policy only

This removes ambiguity but makes common repository-wide exclusions repetitive.
The layered design preserves `.covmap/ignore` as a hard safety policy and
allows one global positive default for the common case.

### Union global and specific include

Union prevents a covering from narrowing the global default, which is the
primary reason to add covering-specific scope. Replacement gives a complete,
locally readable positive policy for that covering.

### Let specific ignore override global ignore

That would weaken repository-wide exclusions for secrets and generated
material. Additive ignore keeps the global policy monotonic and easy to audit.

### Exact paths only

Root-anchored exact paths already provide an explicit manifest. Directory and
glob patterns additionally admit maintained file families without another
format.

### General covering rename instead of migration sanitization

A general `covering rename` operation would need to coordinate the covering
ref, `HEAD`, selection policy, local connection sides, iterator metadata, and
diagnostics for inbound connections stored in repositories it cannot
discover. That is useful but substantially broader than repairing the unsafe
legacy namespace.

The migration-only deterministic rewrite solves the compatibility dead end
without claiming those general lifecycle semantics. It updates every local
stored reference, gives external repositories the same old-name-to-new-name
mapping, and provides `--recover-covering` when the original escaped spelling
is otherwise absent from metadata. General user-directed covering rename and
its cross-repository workflow remain deferred.

### `.covmap/track`

`track` is ambiguous after CIP 0010 because it may mean Git tracking rather
than covering membership. `include` and `selections` describe discovery
without colliding with the publication vocabulary.

### Synchronize coverings to policy

Automatic removal would discard segmentation and covering-local metadata,
break hash endpoints, affect connection coverage, and leave frozen iterators
referring to historical objects. File removal needs its own explicit command,
connection diagnostics, and metadata policy.

## Implementation outline

- Introduce parsed pattern records retaining text, source path, and line
  number.
- Retain the existing matcher as `entry_match` and add ancestor-aware
  `file_match`.
- Land `valid_covering_name` and validated covering-path construction first as
  a format-independent security fix. Make it at least as strict as the
  existing segment-name delimiter/sigil rules, apply it to every command and
  stored covering-name field, and add no selection-policy dependency to that
  patch.
- Add a covering-scoped `SelectionPolicy::load(repo, covering)` implementing
  global fallback, specific-include replacement, and additive ignores.
- Add repository helpers for global include and
  `selections/<covering>/{include,ignore}`.
- Resolve `HEAD` before policy lookup in `cmd_cover`.
- Refactor walking to accept an effective policy and return per-pattern match
  accounting for stable diagnostics.
- Continue pruning directories through the union of global and
  covering-specific ignores; never prune only because include did not match
  the directory entry.
- Compare `Covering::covered_files()` with the effective policy and emit the
  bounded sticky warning without mutating the covering.
- Treat selection policy as tracked project metadata in repo layout comments,
  init output, README, theory, requirements, glossary, skill, and tutorial.
- Narrow the legacy generated `.covmap/.gitignore` `*.tmp` rule to the reserved
  `\#covmap-tmp-*.tmp` atomic namespace so valid `.tmp`-suffixed covering refs
  remain tracked; preserve custom publication policies with a warning.
- Extend common `fsck` validation, including `--offline`, to require readable
  policy files and reject invalid selection-layout entries without requiring
  a covering ref to exist yet.
- Bump `FORMAT_VERSION` to `4`; chain formats 1 and 2 through format 3, add the
  staged metadata-only 3 to 4 phase, apply `migrate_name` consistently to
  covering refs and every stored covering-name reference, publish the version
  last, and give every older supported version migration guidance derived from
  `FORMAT_VERSION`.
- Make the legacy name-normalization stage journaled and resumable before the
  existing format-1 or format-2 migration phase; migration-specific recovery
  must reject symlinks and any source outside `.covmap`.
- Add repeatable `migrate --recover-covering <legacy-name>` solely to identify
  an unreferenced escaped legacy ref; derive its destination through
  `migrate_name` and persist the recovery input in the migration manifest.
- Keep covering, connection, cache, peer, and iterator encodings unchanged.
- Keep the `cover` command grammar unchanged.

## Tests

### Matching and precedence

- Existing `.covmap/ignore` fixtures select the same files after migration.
- Missing include means all otherwise-discoverable files.
- Empty global include selects none for coverings without a specific include.
- A specific include replaces global include.
- Empty specific include selects none and does not fall back.
- Global and specific ignore patterns form a union.
- Global ignore vetoes a path admitted by a specific include.
- Pattern order does not affect selection.
- Anchored and unanchored directory patterns use ancestor matching.
- A deeper file glob matches even when its parent directory does not.
- Dot-prefixed entries and symbolic links remain undiscoverable.

### Multiple coverings and mappings

- Two coverings in one repository admit disjoint file sets from distinct
  specific include policies.
- A covering without a specific include uses global include while another
  covering overrides it.
- `cover HEAD` uses the policy for the covering named by `.covmap/HEAD`.
- A same-repository connection between differently scoped coverings resolves
  and computes coverage from their actual refs.
- External peer binding and connection behavior are unchanged by policy files.

### Additivity and diagnostics

- A selected new text file receives one whole-file `unknown` segment.
- Repeating `cover C` is idempotent.
- Widening effective include adds new files without touching old entries.
- Narrowing include or widening ignore leaves old entries byte-for-byte
  unchanged and emits the sticky covering-specific policy warning.
- Every unmatched active include pattern warns independently.
- Inactive global include patterns do not warn for a covering with a specific
  include.
- Include patterns matched before an ignore veto are not reported unmatched.
- Empty effective include emits no unmatched-pattern warning.
- Selected binary and empty files retain current handling.

### Project artifact and compatibility

- `covmap init` creates format 4 and a selection-policy directory or supports
  its lazy creation.
- The generated `.covmap/.gitignore` does not exclude global or
  covering-specific selection policy.
- Ordinary `git add .covmap` includes populated selection policy files while
  continuing to exclude cache, local bindings, iterators, HTML, and temporary
  files.
- Ordinary `git add .covmap` includes a valid covering ref such as
  `coverings/release.tmp` while excluding `#covmap-tmp-*.tmp` atomic files.
- A clean clone receives the same policy and produces the same candidate set.
- Full and offline `fsck` reject unreadable policy, non-file policy entries,
  and unexpected names inside one covering's selection directory, while
  allowing policy for a covering that has not been created yet.
- Every covering-name entry point rejects `.`, `..`, `HEAD`, separators, NUL,
  Unicode whitespace, `:`, and a leading `@` or `#` before side effects;
  traversal attempts create no file inside or outside the repository.
- `cover foo:bar` is rejected without creating a ref, while representative
  valid covering names round-trip through `cover`, explicit `<C>:<address>`
  operands, and both sides of `connect`.
- `fsck` rejects unsafe covering refs, selection directories, `HEAD` targets,
  connection endpoints, and iterator covering metadata.
- `fsck` accepts a valid `selections/<C>/` whose covering ref does not yet
  exist.
- Format-3 migration preserves pre-existing policy, creates no policy when
  absent, and retains ignore-only behavior.
- Format-3 migration deterministically escapes unsafe names and names already
  beginning with `covmap-escaped-`, rewriting covering refs, `HEAD`,
  connection fields, iterator metadata, and selection directories together.
- Legacy names containing `:`, any whitespace, or a leading `@` or `#` use
  the same deterministic escape path as legacy path-unsafe names.
- Long legacy names produce one bounded full-SHA-256 escaped component;
  distinct-name output collisions are rejected before publication.
- Two independently migrated repositories derive the same rewritten name for
  an unsafe external connection endpoint, including when the peer is unbound.
- Migration recovers a referenced legacy `../evil` ref only when its old file
  is proven to be a non-symlink regular file within `.covmap/`; a symlink or
  outside-`.covmap` traversal target is never read, moved, or deleted.
- `migrate --recover-covering ../evil` repairs an otherwise-unreferenced
  `.covmap/evil` legacy covering, resumes without repeating the option after
  interruption, and rejects nonexistent, ambiguous, reserved, symlinked, or
  outside-`.covmap` sources without live changes.
- Any migration validation or recovery failure leaves the repository at
  format 3 with all live artifacts unchanged.
- Formats 1 and 2 resumably normalize unsafe names before their existing
  migration phases, then migrate through format 3 to format 4.
- Format-3 rejection explicitly directs the user to `covmap migrate` and names
  the current supported format without a hard-coded version-3 hint.
- Format-3 binaries reject format 4 before running `cover`.

## Deferred

- An explicit `uncover` operation and connection consequences.
- Exact reconciliation or a `cover --sync` mode.
- Covering-specific override of global ignore.
- A future covering-rename operation and its selection-directory lifecycle.
- Inclusion of explicitly named dot-prefixed paths.
- Full gitignore-compatible syntax.
- A read-only policy explanation or dry-run command.

## Landing checklist

This is a covering-discovery and project-artifact change. Per
`DEVELOPER.md`, land it in artifact order:

- **Theory:** define repository and covering-specific selection policy,
  distinguish it from `F(C)`, and add it to the portable project-artifact
  boundary.
- **Requirements:** amend req7, req17, req18, and req24 for positive
  selection, covering-specific precedence, format 4, and tracked policy.
  Req6 remains unchanged because covering refs still declare the actual
  covered set.
- **Implementation:** add policy loading and matching, per-covering
  precedence, diagnostics, offline verification, format-4 migration, and
  focused unit and smoke tests.
- **Skill:** replace ignore-only setup guidance with global/default and
  covering-specific selection workflows; correct the existing implication
  that tightening policy and re-covering removes entries; extend T1 and T12
  additively without replacing CIP 0010's portability coverage or adding
  overlapping tasks.
- **Tutorial:** demonstrate two differently scoped coverings in one
  repository, commit their policy with the project artifact, and show that
  narrowing policy does not remove existing segments.
- **README / glossary:** distinguish covering-input selection from
  `.covmap/.gitignore`, document the format-4 layout and precedence, and define
  global and covering-specific selection policy.
- **Self-map:** remap every amended requirement and run `covmap status`,
  `covmap fsck --offline`, and full `covmap fsck` with required peers bound.
