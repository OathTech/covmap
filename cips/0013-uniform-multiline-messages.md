# Uniform multiline messages

Stage: **Accepted**

## Relationship to landed CIPs

[CIP 0002](0002-segment-messages.md) established repeatable
`-m` / `--message` values as blank-line-separated paragraphs for segment
messages. [CIP 0001](0001-iterators.md) introduced task notes, and
[CIP 0006](0006-mapping-review-surfaces.md) explicitly kept link notes
single-line because connection refs and plumbing output were line-oriented.

This proposal extends CIP 0002's input convention to every message-bearing
command. It supersedes only CIP 0006's single-line link-note constraint.
Segment, link, and task semantics remain distinct; their command-line message
input becomes uniform. Uniformity applies to explicit message input and
storage, not to what each command does when no message flag is supplied.

## Idea

Give every message-bearing command the same repeatable grammar:

```text
covmap annotate <C>:<addr> [-m <text>]...
covmap link <K> <A> <B> [-m <text>]...
covmap iter <I> done [-m <text>]...
covmap iter <I> skip [-m <text>]...
```

Each `-m` / `--message` supplies one paragraph. Multiple values are joined
with one blank line, and a single value may itself contain newlines. The
result replaces the complete message for that invocation; repeated flags do
not append to an already stored message.

This intentionally follows the established `git commit -m` convention:
repeating `-m` supplies separate paragraphs in command-line order.

Make link notes and iterator task notes safely multiline by storing every
non-empty note in a length-prefixed stanza immediately after its owning
record. This follows the existing segment-message body design without copying
its index-based association. Preserve one-physical-line plumbing records by
escaping control characters in note fields.

## Motivation

The current interface has one flag name but two arity rules:

- `annotate` accepts repeated `-m` values and joins them as paragraphs;
- `link`, `iter done`, and `iter skip` accept at most one value.

Before the shared parser refactor, iterator resolution silently kept only the
last repeated value. Rejecting repeats prevents that data loss, but it leaves
the commands inconsistent and prevents practical multi-paragraph mapping
rationales and audit verdicts.

These notes have concrete review uses. A link rationale can state the mapping
claim in one paragraph and its evidence, limitation, or review boundary in a
second. An iterator verdict can record the outcome separately from validation
performed or remaining risk. Because connection refs are Git-tracked, keeping
those paragraphs as literal lines also makes rationale edits visible as normal
prose diffs rather than escaped string changes.

The storage layer is also unsafe today. Link and task notes are written
directly into final TAB fields. A single argument containing a newline can be
accepted by the CLI, written across physical lines, and then make the ref
unreadable. Repeatable parsing cannot be enabled safely without changing
storage and plumbing at the same time.

## Terminology

The domain names remain:

- **segment message**: durable rationale about one segment;
- **link note**: rationale about one mapping pair;
- **task note**: verdict from one iterator pass.

`-m` / `--message` is the common input mechanism for all three. This proposal
does not rename link notes or task notes to segment messages.

## Unified command contract

### Parsing

For every message-bearing command:

1. `-m` and `--message` are interchangeable and may be mixed.
2. Every occurrence requires exactly one following value.
3. Values remain in command-line order.
4. One value supplies the complete initial text before normalization.
5. Several values are joined with `"\n\n"`.
6. Embedded newlines inside one value are preserved.
7. Missing values, positional message text, and unrelated arguments fail
   before mutation.

Examples:

```sh
covmap annotate main:@parser \
  -m "Parses one logical record." \
  -m "Kept separate because its error policy maps independently."

covmap link spec_to_impl @parse-record @decode-record \
  -m "The spec parser normalizes escaped fields." \
  -m "The implementation validates the decoded representation."

covmap iter audit done \
  -m "Implemented by decode_record()." \
  -m "Negative cases are covered by parser_errors."
```

Each command stores two paragraphs separated by one blank line.

### Replacement and clearing

Message input replaces the complete stored value. It never appends to the
previous value.

The canonical cross-command clearing gesture is:

```text
-m ""
```

An explicit message input whose normalized result is empty clears the stored
message or note. Empty leading values remain leading blank paragraphs when a
later value is non-empty. Empty trailing values are removed by normalization.

`annotate <addr> -` remains accepted as a compatibility alias for clearing,
but examples and generated guidance should use `-m ""`. Link and iterator
commands intentionally do not gain a bare `-` alias.

### Meaning of no message flag

An absent message input is not the same as an explicitly empty input. The
command determines what absence means:

| Command | No `-m` / `--message` |
| ------- | --------------------- |
| `annotate <addr>` | Read and print the current segment message. |
| `link <K> <A> <B>` for a new pair | Create the link with no note. |
| `link <K> <A> <B>` for an existing pair | Preserve the existing note. |
| `iter <I> done` / `skip` | Resolve with no task note, clearing an earlier verdict. |

These behaviors already follow each command's operation. Uniformity applies
to how an explicitly supplied message is parsed and stored, not to turning
every command into the same operation.

### Normalization

Use one shared normalization rule:

- join repeated values with one blank line;
- normalize CRLF and bare CR line endings to LF;
- strip all trailing LF characters from the joined value;
- treat an empty normalized value as absent storage;
- otherwise preserve all text, including blank lines, leading spaces, tabs,
  `#`-prefixed lines, and Unicode.

Stripping all trailing LFs makes normalization idempotent:
`normalize(normalize(x)) == normalize(x)`. Generated repair commands can
therefore feed stored text back through one `-m` without changing it. This
intentionally tightens the existing segment-message rule, which stripped only
one trailing LF. Existing stored segment messages remain byte-for-byte
unchanged until explicitly replaced.

The pipeline is normative:

| Supplied values | Joined value | Stored value |
| --------------- | ------------ | ------------ |
| `[""]` | `""` | absent (clear) |
| `["", ""]` | `"\n\n"` | absent (clear) |
| `["a", ""]` | `"a\n\n"` | `"a"` |
| `["", "a"]` | `"\n\na"` | `"\n\na"` |
| `["a\r\nb\r\n"]` | `"a\r\nb\r\n"` | `"a\nb"` |
| `["a\nb"]` | `"a\nb"` | `"a\nb"` |

## On-disk format

### Format version and bump policy

This is an incompatible ref-format capability for existing readers.
Increment `FORMAT_VERSION` from 4 to 5.

A format-4 reader sees note stanza bodies as record lines and can reject or
misinterpret them. Format 5 therefore uses an eager repository-wide bump,
rather than waiting for the first multiline write. Migration rewrites every
non-empty inline connection and iterator note into the canonical format-5
stanza form and publishes `version=5` last.

The cost is deliberate: old binaries are locked out even when a repository has
no notes, and repositories with existing notes receive a one-time textual
rewrite. In return, every format-5 ref has one canonical grammar, readers never
need to infer capability from individual files, and an old writer cannot
silently reintroduce format-4 inline notes. A lazy bump would make repository
compatibility depend on write history and is rejected.

The project explicitly chooses the future value of uniform, Git-reviewable
multiline rationales over minimizing migration work for the current installed
base. Format 5 and always-stanza notes are therefore decisions in this proposal,
not unresolved cost questions. The migration will produce a one-time diff
proportional to the existing note corpus, including notes in covmap's own
tracked connections; that diff is expected.

### Shared length-prefixed text bodies

Covering messages already use an indexed stanza:

```text
# message <index> <line-count>
<body line 1>
<body line 2>
...
```

Connections and iterators instead use an adjacent stanza:

```text
# note <line-count>
<body line 1>
<body line 2>
...
```

`<line-count>` makes blank lines, TABs, record-looking lines, and
`#`-prefixed lines unambiguous. Implement one internal length-prefixed body
reader/writer, while leaving association to each container: coverings retain
their existing index, and connection/iterator notes belong to the immediately
preceding record.

The note header intentionally carries no index, type, author, timestamp, or
other speculative metadata. A demonstrated need for note metadata should use a
later format revision rather than pre-generalizing format 5.

An orphan note stanza, a second stanza for one record, a malformed line count,
or a truncated body is an error.

### Connection refs

Keep the existing metadata and sort complete link blocks by endpoint pair:

```text
# source_covering=main
# target_peer=impl
# target_covering=main
@parse-record	@decode-record
# note 1
Short single-line note.
@validate-record	@check-record
# note 3
First paragraph.

Second paragraph.
```

Format-5 link rows contain only the two endpoint fields. Every non-empty note,
including a single-line note, uses one immediately following stanza. An empty
note has no stanza. The row and its optional stanza form one record block.
Endpoint pairs are unique, so serialization sorts blocks by `(A, B)` only; the
note is not part of the comparator.

This representation is intentionally more verbose than an inline field. A note
edit changes the stanza body directly, changing a note from one line to several
does not switch representations, and inserting an earlier sorted link does not
renumber or rewrite later note headers. These properties make Git diffs and
merges more stable.

### Iterator refs

Task rows remain in their frozen order:

```text
# covering=main
# order=file
# cursor=0
<hash>	<file>	<a>	<b>	<label>	<name>	done
# note 3
Implemented by decode_record().

Negative cases covered.
```

Format-5 task rows contain the existing fields through task state, but no
inline note field. Every non-empty note uses one immediately following stanza.
An empty note has no stanza.

Iterator order, state, cursor, and enumeration semantics are unchanged.

### Covering refs

Covering message storage remains byte-for-byte unchanged. The implementation
may refactor it to use the shared length-prefixed body codec, but the serialized
`# message <index> <line-count>` form must not change.

## Migration

`covmap migrate` accepts published versions 1 through 4, recognizes its
`migrating-4-to-5` transitional state, and ends at version 5:

```text
1 -> 2 -> 3 -> 4 -> 5
```

The 4-to-5 phase reuses the existing ready-manifest migration pattern. Its
local state lives under `.covmap/local/migrate-4-to-5/` and contains a complete
staged connection/iterator snapshot plus a manifest inventory.

On every `migrate` invocation, the orchestrator checks both the published
version and this journal before ordinary version dispatch:

- version 4 without a ready manifest starts or restarts staging;
- version 4 with a ready manifest means publication never started, so the
  stale prepared snapshot is discarded and rebuilt from the still-format-4
  live refs;
- transitional version `migrating-4-to-5` with a ready manifest resumes
  format-5 publication from staged bytes;
- version 5 with a ready manifest is stale completed state and removes it;
- the transitional version without a valid ready manifest is an error and
  never guesses at recovery.

Without a ready manifest, the phase:

1. loads every connection and iterator ref with line-aware format-4
   validation;
2. preserves each decoded note value, including valid TAB characters;
3. serializes the complete canonical format-5 ref set into the staging
   directory, rewriting every non-empty inline note as an adjacent stanza;
4. atomically writes a `ready` manifest only after every staged file exists and
   the destination inventory is complete;
5. atomically changes the version file from `4` to `migrating-4-to-5` before
   replacing any live ref.

Once the transitional version is published, the ready manifest is the
authoritative publication plan:

6. every ordinary command refuses with unfinished-migration guidance; older
   binaries also reject the unknown transitional version;
7. `migrate` validates the manifest and staged snapshot, then atomically
   replaces each destination file in deterministic order;
8. re-running repeats those replacements idempotently from staged bytes rather
   than parsing any already-published format-5 ref as format 4;
9. after every destination has been published, migration atomically writes
   `version=5`;
10. only then does it remove the journal and staging directory.

A crash before the ready manifest leaves live refs untouched, so the incomplete
stage can be discarded and rebuilt. A crash after the manifest but before the
transitional version also leaves live refs untouched; the next migration
restages conservatively in case a format-4 command changed them. A crash after
the transitional version may leave a mixture of physical ref encodings, but
both old and new ordinary binaries are gated and migration resumes from the
complete staged snapshot. A crash after version 5 but before cleanup is
completed by the format-5 terminal path. This is logical ref-set atomicity
through a resumable transaction; it does not assume a filesystem primitive
that renames several files atomically.

Migration assumes no other covmap process is mutating the repository
concurrently. Crash recovery and conservative restaging handle sequential
interleavings, including an old format-4 binary running between migration
attempts, but the local CLI does not provide a cross-process repository lock.

The `migrating-4-to-5` name intentionally establishes a reusable
`migrating-<from>-to-<to>` transitional-version convention for future format
bumps.

The migration changes representation, not logical note values. Repositories
with no connection or iterator notes need only the version publication.
Format-4 TABs are valid because the note is the final split field; migration
moves them into stanzas. CR and LF are ref-safety concerns because they can
create physical records, while TAB escaping is principally required by
line-oriented plumbing.

The migration orchestrator must treat format 4 as a phase rather than its
terminal early-return case. Formats 1 through 3 chain through format 4 and then
format 5 in the same invocation. Current terminal policy repair and stale-state
cleanup move to the format-5 path.

If migration encounters a physical line that cannot be parsed as a format-4
record, it must fail without mutation and name the file and offending line. It
must not guess which malformed lines are continuations. A raw multiline note
whose continuation happens to be a syntactically valid record is
indistinguishable from two legitimate adjacent format-4 records and therefore
cannot be detected universally; such pre-existing corruption migrates as the
records represented on disk. Recovery from a diagnosed malformed line is from
version control or an explicit manual repair followed by migration. Existing
connection and iterator parse errors do not all carry physical line numbers,
so line-aware migration diagnostics are explicit implementation work.

## Rendering and plumbing

### Porcelain

`show`, connection views, iterator detail, and HTML render decoded text with
paragraph boundaries intact.

Connection-note wrapping becomes line-aware:

- wrap each non-empty source line to the existing width;
- preserve explicit line breaks;
- preserve empty lines between paragraphs;
- indent every rendered note line beneath its link heading.

HTML uses pre-wrapped text so stored newlines remain visible.

### Line-oriented output

`ls-links`, `ls-links --resolve`, and `iter peek --all` must remain one
physical line per record. Escape the final note field with one shared codec:

| Stored byte | Output |
| ----------- | ------ |
| `\` | `\\` |
| TAB | `\t` |
| CR | `\r` |
| LF | `\n` |

Plain notes containing none of these bytes remain byte-for-byte unchanged.
The escape order processes backslash first, making the result unambiguous.
Document this field encoding as part of the plumbing contract.

Other structured renderers, including HTML tooltip JSON, receive decoded text
and apply their own established escaping.

## Generated repair commands

Rename and subdivision repair commands must preserve multiline link notes.
Use the existing POSIX shell-quoting helper with one `-m` argument containing
the complete decoded note. POSIX single quotes may span physical lines, so the
delimited repair block remains executable.

Generated commands are not required to reconstruct the original division
between repeated `-m` flags; only the normalized stored text must round-trip
byte-for-byte.

Empty notes continue to omit `-m` in a generated command for a new replacement
pair.

## Implementation outline

- Replace `parse_single_message` with one shared message-input parser returning
  either absent input or a normalized complete value.
- Use that parser from `annotate`, `link`, `iter done`, and `iter skip`.
- Keep command-specific absent-input behavior in each caller.
- Extract the length-prefixed body reader/writer from covering messages; retain
  indexed association for coverings and add adjacent association for notes.
- Add line-aware format-4 validators and a format-5 migration after the
  existing format-4 phase.
- Make format-5 connection and iterator serializers emit note-free record rows
  followed by a stanza for every non-empty note.
- Sort connection record blocks by endpoint pair only.
- Reuse the ready-manifest staging pattern for the complete format-5 ref set;
  add the `migrating-4-to-5` transitional version and resume it before parsing
  any live ref.
- Make every ordinary command recognize the transitional version and refuse
  with guidance to rerun migration; older binaries refuse it as unsupported.
- Make format 5 the terminal migration branch; chain older formats through the
  new 4-to-5 phase and perform terminal cleanup there.
- Add the shared line-field escaping codec to link and iterator plumbing.
- Make connection note wrapping preserve explicit paragraphs.
- Publish each staged ref idempotently, publish the version file last, and
  clean staging only after version 5 is durable.

## Invariants

- Every explicit message input is parsed identically across all commands.
- Repeated values are ordered paragraphs, never last-value-wins.
- One invocation replaces one complete message; it never implicitly appends to
  stored text.
- Empty explicit input clears; absent input remains distinguishable.
- Normalization is idempotent, and newly stored message text never ends in LF.
- Every normalized message body round-trips through its ref, including internal
  blank lines, leading blank lines, tabs, `#`-prefixed lines, and Unicode.
- A connection ref retains exactly one note per endpoint pair.
- An iterator retains exactly one note per task.
- Every non-empty format-5 link or task note is adjacent to its owning record;
  its association never depends on a mutable record index.
- Plumbing emits exactly one physical line per item.
- Existing valid format-4 notes, including TABs, survive migration
  byte-for-byte in memory.
- Before `migrating-4-to-5` is published, migration has not changed live refs.
- While `migrating-4-to-5` is published, a valid ready manifest exists,
  ordinary commands refuse, and rerunning migration completes the authoritative
  staged snapshot without parsing mixed live refs.
- Version 5 is published only after every staged ref has been replaced; stale
  post-publication journal state is cleanup-only.

## Tests

- A shared parser accepts short and long flags in mixed order and joins three
  values with blank lines.
- The normalization table cases pass exactly, including multiple empty values,
  trailing empty paragraphs, leading empty paragraphs, CRLF, and bare CR.
- Normalization is idempotent for stored output.
- All four command shapes store the same normalized result for the same
  explicit message arguments.
- Positional message text and a missing flag value fail before mutation.
- `-m ""` clears segment, link, and task text.
- `annotate -` remains a clearing alias.
- Bare annotate reads, bare re-link preserves, and bare iterator resolution
  clears according to the absence table.
- Connection and iterator refs round-trip notes containing blank lines, tabs,
  `#`-prefixed lines, backslashes, apostrophes, and Unicode.
- Format-5 serializers use adjacent stanzas for both single-line and multiline
  notes and never write inline note fields.
- Inserting an earlier sorted link does not rewrite or renumber later note
  associations.
- Editing a note does not affect connection block ordering; the sort key is
  exactly `(A, B)`.
- Orphan, duplicate, malformed-line-count, and truncated stanzas fail clearly.
- Note bodies containing complete header-looking lines such as `# note 2` and
  `# message 0 1` round-trip as body text and cannot spoof structure.
- `show` preserves paragraph breaks while wrapping long lines.
- HTML preserves paragraph breaks and escapes markup.
- Link and iterator plumbing each emit one record with reversible escaped note
  fields.
- Generated repair commands reproduce a multiline note without executing
  shell metacharacters.
- A format-4 repository with inline single-line and TAB-containing notes
  migrates them to adjacent stanzas without changing logical state.
- A format-4 repository with no notes publishes version 5 without unnecessary
  ref rewrites.
- A format-4 ref with a physical line that is not a valid record blocks
  migration without publishing version 5 and reports its file and physical
  line.
- A crash before the ready manifest leaves all live refs byte-for-byte
  unchanged and a rerun restages from format 4.
- A crash after the ready manifest but before the transitional version leaves
  live refs unchanged; a rerun discards and rebuilds the prepared snapshot.
- A crash after publishing `migrating-4-to-5` and any subset of staged refs
  makes old and new ordinary commands refuse; rerunning migration republishes
  the full snapshot and reaches format 5.
- The transitional version without a valid ready manifest fails clearly
  without touching live refs.
- A crash after version 5 but before journal cleanup is recovered as terminal
  cleanup without rewriting refs.
- Formats 1, 2, and 3 chain through the 4-to-5 phase in one invocation, while a
  format-5 repository runs only terminal policy and stale-journal cleanup.
- Smoke and tutorial workflows exercise repeated messages on segment, link,
  and iterator commands.

## Alternatives rejected

### Keep link and iterator messages single-value

This preserves the current line format but leaves one shared flag with
different arity and prevents practical multi-paragraph rationales.

### Silently keep the last value

This was the old iterator behavior. It loses user input and makes flag order an
implicit overwrite operation.

### Join repeated values with spaces

This avoids storage work but destroys paragraph structure and differs from the
established `annotate` convention.

### Write raw newlines into TAB fields

This creates extra physical records and makes connection and iterator refs
unreadable.

### Escape durable notes into one field

A bespoke inline escaping grammar keeps physical records compact but makes
tracked refs harder to review and requires a compatibility marker to
distinguish encoded text from legacy literal backslashes. Length-prefixed
stanzas already solve these problems for segment messages.

### Mix inline and stanza notes

Keeping simple notes inline reduces the migration diff, but changing one note
from single-line to multiline also changes its representation. It leaves two
canonical write shapes and preserves TAB-specific edge cases. One stanza form
for every non-empty note is more predictable.

### Use indexed note stanzas

The existing covering-message format associates stanzas by record index, but
connection rows are sorted and links can be inserted or removed. An insertion
near the start would renumber every later multiline note header, creating
unrelated Git churn and merge conflicts. Adjacent stanzas preserve locality
while still reusing the length-prefixed body codec.

### Stage parsing before multiline storage

Landing repeatable parsing while retaining format-4 note fields would either
reject the newly accepted paragraphs before mutation or write raw newlines into
refs. The `link -m` alignment and shared scanner can land as groundwork, but
the repeatable link/task contract must land atomically with safe storage and
plumbing.

### Keep format version 4

Old format-4 readers cannot safely consume note stanza bodies. The repository
version must expose that incompatibility before a command reads or rewrites
refs.

### Bump the version lazily

Publishing format 5 only on the first note write avoids locking old binaries
out of untouched repositories, but makes compatibility depend on repository
history and permits mixed canonical forms. An eager, transactionally resumable
migration gives the repository one explicit capability boundary.

### Keep version 4 published while replacing refs

A ready local journal lets the new migrator resume, but an old format-4 binary
does not know that journal exists and could read or mutate a partially converted
repository. Publishing `migrating-4-to-5` before the first ref replacement
makes the version file an effective gate for both old and new binaries.

## Review points

- Confirm that stripping all trailing LFs is the right idempotent normalization
  rule for segment messages, link notes, and task notes.
- Validate the ready-manifest and transitional-version state machine at every
  crash boundary.
- Confirm that `-m ""` is canonical clearing while only `annotate` retains the
  compatibility `-` alias.
- Confirm the proposed control-character escaping contract for line-oriented
  plumbing.

## Landing checklist

- **Theory:** make link and iterator note domains arbitrary text rather than
  single-line scalars; update the command interface and format version.
- **Requirements:** amend req10 (link notes), req17/req24 (versioned durable
  refs and migration), req19/req20 (escaped plumbing and rendering), req21
  (task notes), and req22 (shared message input convention).
- **Implementation:** add shared parsing, length-prefixed body and line-field
  codecs, adjacent-note serializers, line-aware format-4 validation, format-5
  journal/resume orchestration, renderers, and unit tests.
- **Skill:** document one repeatable message convention, clearing, multiline
  review, and task/link examples.
- **Playground:** extend message, link, and iterator tasks with repeated
  message values and multiline inspection.
- **Tutorial:** demonstrate two-paragraph segment, link, and task text and
  verify their rendered and plumbing forms.
- **README / glossary:** document the common grammar, three text domains,
  format 5, migration, and escaped line fields.
- **Smoke tests:** cover parser parity, multiline ref round-trips, migration,
  repair commands, rendering, and one-line plumbing.
- **Self-map:** remap amended requirements/theory and run `covmap status`,
  `covmap fsck`, migration fixtures, smoke, and tutorial gates.
