# covmap tutorial

A guided, hands-on tour of every main covmap feature, end to end. It
maps a small Markdown **spec** to a Rust **implementation**, then edits
the spec three different ways to exercise covmap's drift repair — the
names, diff-remap, and fingerprinting layers.

This document defines the walkthrough. The companion script
[`run_tutorial.sh`](run_tutorial.sh) derives from it, executes the same
scenario, and asserts the key outcomes, so it doubles as an integration test.
This guide orders concepts for learning; the script runs a few robustness
checks earlier than they are explained here.

```sh
cd covmap/tutorial
./run_tutorial.sh          # build a fresh workspace and run the whole tour
KEEP=1 ./run_tutorial.sh   # same, and keep _work/ for inspection
./run_tutorial.sh clean    # remove the _work/ workspace
```

The script works inside a throwaway `tutorial/_work/` directory and
wipes it at the start of every run, so you can re-run it freely. It
builds `covmap` (release) if the binary isn't present, or honours a
`COVMAP_BIN` you set yourself.

### Following along by hand

First make sure the `covmap` you'll type is the **current** build. Build
and use the fresh one:

```sh
cd covmap
cargo build --release
export PATH="$PWD/target/release:$PATH"   # this shell now uses the fresh covmap
covmap --version
```

(If you've `cargo install`ed covmap before, re-run `cargo install --path
. --force` to refresh `~/.cargo/bin/covmap` — note the version string may
be unchanged even when the behavior isn't.)

Then work from the **fixture files** in [`files/`](files/) rather than
the post-run `_work/` (whose sources are already in their *final*,
drifted state). Set up a scratch copy under `tutorial/_play/` (a sibling
of the script's `_work/`, so the two never collide):

```sh
cd covmap/tutorial
mkdir -p _play/spec _play/impl_rs
cp files/queue.md files/errors.md _play/spec/
cp files/queue.rs                 _play/impl_rs/

# A bit of generated noise, so the ignore step below has something to do:
mkdir -p _play/spec/build
echo "generated junk" > _play/spec/build/artifact.txt

cd _play/spec     # the rest of the tutorial runs from here
```

Almost every step operates on the spec covering, so you stay in
`_play/spec` throughout; the few commands that touch the implementation
reach it as `../impl_rs`.

The source files evolve in three stages, kept as separate fixtures so
each step starts from a known state (no in-place editing magic):

| Stage | File | Used by |
| ----- | ---- | ------- |
| initial | `files/queue.md` | steps 1–11 |
| in-place edit | `files/queue.reworded.md` | step 12 |
| inserted lines | `files/queue.inserted.md` | step 13 |
| moved section | `files/queue.moved.md` | step 14 |

(Stage files are named for the *edit* they represent, not a step number,
so they stay meaningful if the tutorial is reordered.)

At each drift step, "edit the file" means **copy the next stage over
`queue.md`** — e.g. `cp ../../files/queue.reworded.md queue.md` (run
from `_play/spec`, where you are). That is exactly what the script does,
so your hashes and line numbers will match this document. When you're
done, `rm -rf tutorial/_play` to clean up.

For the agent-facing command and workflow reference, see
[`../SKILL.md`](../SKILL.md).

---

## The workspace

Whether you run the script (`_work/`) or follow along by hand
(`_play/`), the layout is the same:

```
<workspace>/
  spec/                     # repo A — the specification
    queue.md                # a bounded FIFO queue spec, three operations
    errors.md               # error codes
    build/artifact.txt      # generated noise we will ignore
  impl_rs/                  # repo B — the implementation
    queue.rs                # the Rust queue
```

`spec/queue.md` has three `##` sections (Enqueue, Dequeue, Size) under a
preamble; `spec/errors.md` lists two error codes.

---

## 1. Initialize, select, cover

Each repository gets its own `.covmap/`. **Before the first `cover`**, define
the source-of-truth set positively. The repository include selects the files
that matter; ignore is a hard veto for generated or irrelevant paths.

```sh
covmap init                          # in _play/spec
(cd ../impl_rs && covmap init)

printf '/queue.md\n/errors.md\n' > .covmap/include
printf 'build/\n' > .covmap/ignore
printf '/queue.rs\n' > ../impl_rs/.covmap/include

covmap cover
(cd ../impl_rs && covmap cover)
```

`cover` adds one whole-file `unknown` segment per selected readable file it
has not yet covered — this first, unrefined covering is the *initial cover*.
It is additive and idempotent. `build/artifact.txt` is absent because the
global ignore vetoes it.

Selection affects future discovery only. Narrowing it later does not remove
an existing covered path; `cover` warns and leaves the ref unchanged. A
present empty include selects no new files. Active include patterns that match
no discoverable file warn with their policy file and line number.

```
Covering 'main': 2 segments, 20 lines (2 added)
```

## 2. Inspect the covering

```sh
covmap status
covmap ls -l
covmap show main:queue.md
```

```
covering main: segments=2 lines=20
  drift: clean
238857a21a1ee053	unknown	errors.md:1..4	4	-
c7da83ee193093c5	unknown	queue.md:1..16	16	-
region main queue.md · 1 segment(s)
  c7da83ee193093c5  unknown  queue.md:1..16
    (unlinked)
--- content c7da83ee193093c5 ---
 1 | # Task Queue Spec
 2 |
 3 | A bounded FIFO queue of tasks, processed oldest-first.
 4 |
 5 | ## Enqueue
 6 |
 7 | `enqueue(t)` appends task t to the back. It must reject the task with a
 8 | QueueFull error when the queue is already at capacity.
 9 |
10 | ## Dequeue
11 |
12 | `dequeue()` removes and returns the front task, or None when empty.
13 |
14 | ## Size
15 |
16 | `size()` returns the number of queued tasks, always within capacity.
```

`status` summarizes the covering and its drift. `ls -l` gives one compact row
per segment: hash, label, location (`file:a..b`), line count, and name (`-`
when anonymous). `show main:queue.md` is the human inspection view: an
explicit file selector presents the file as ordered covmap segments, each with
its relationships and numbered recorded content. Add `--no-content` when you
need only the compact relationship view.

That is enough internal state for the normal workflow. Keep using the
porcelain commands below; the optional cache and plumbing details come after
the iterator section.

## 3. Refine into one segment per section

There are no automatic segmentation rules — you refine by hand with
`split`. A file selector such as `main:queue.md` inserts several cuts
atomically against the covering state present when the command starts.
You can instead point at one segment by hash, `@name`, or position:
`main:queue.md:5` means *"the segment of `queue.md` that covers line 5"*.
Exactly one segment covers any given position.

The `##` headings sit at lines 5, 10, 14 (the Enqueue body wraps onto
two lines, so the sections are *not* uniformly sized — read the line
numbers off the file, don't assume them). Give those starts as one cut
set. The deliberately unsorted operands demonstrate that covmap applies
them in file order and replaces the covering ref once:

```sh
covmap split main:queue.md 14 5 10
covmap label main:queue.md:1  preamble
covmap label main:queue.md:5  Enqueue
covmap label main:queue.md:10 Dequeue
covmap label main:queue.md:14 Size
covmap ls -l
```

`split` changes geometry only: all resulting fragments begin
`unknown`. `label`, `name`, and `annotate` compose metadata explicitly
afterward. Result:

```
preamble	queue.md:1..4	4	-
Enqueue	queue.md:5..9	5	-
Dequeue	queue.md:10..13	4	-
Size	queue.md:14..16	3	-
```

(Hashes omitted above for readability — yours will match a fresh run,
since identity is content-addressed, but you never need to type them.)

## 4. Refinement is reversible: merge, then re-split

Merge the two adjacent segments (Enqueue covers line 5, Dequeue covers
line 10), then split the result — which now covers line 5 — back at
line 10:

```sh
covmap merge main:queue.md:5 main:queue.md:10 "Enq+Deq"
covmap split main:queue.md:5 10
covmap label main:queue.md:5 Enqueue
covmap label main:queue.md:10 Dequeue
```

`merge` requires adjacent segments. Splitting the merged byte range at the
original line-10 boundary recreates the same Enqueue and Dequeue byte ranges.
Because identity is content-addressed, those ranges regain their original
hashes exactly; the labels do not affect the hashes.

## 5. Character-level split: isolate a clause inside a line

A boundary need not fall on a line break. `errors.md` line 3 packs a code
and its meaning — *"QueueFull means the queue is at capacity …"*. We
isolate the meaning clause as its own segment by splitting **mid-line**,
at the column where it begins (`<line>:<col>`, 1-indexed inclusive).

`errors.md` is still one whole-file segment. Split it at line 3 so the
QueueFull line is on its own. This time we'll **name** that line
(`@qf-line`) and then split it *by name* — a name is a unique handle, so
`main:@qf-line` is often the most readable way to point at a chunk
you're working on:

```sh
covmap split main:errors.md 3                  # the QueueFull line is now its own segment
covmap label main:errors.md:1 head
covmap label main:errors.md:3 qf-line
covmap name  main:errors.md:3 qf-line          # give it a stable name
covmap split main:@qf-line 3:17
covmap label main:@qf-line QueueFull-code
covmap label main:errors.md:3:17 QueueFull-meaning
covmap ls -l
```

The cut argument carries the column too: `3:17` cuts at character 17 of
line 3 — where "the queue is at capacity" begins.

```
head	          errors.md:1..2	    2	-
QueueFull-code	  errors.md:3..3:16	1	@qf-line
QueueFull-meaning errors.md:3:17..4	2	-
```

Three things to notice. The locations `errors.md:3..3:16` and
`errors.md:3:17..4` are character-precise: the cut is between columns 16
and 17. When split output says metadata was **inherited**, it means the
first fragment retained the parent segment's name and message. Labels are
not inherited: every fragment starts `unknown`, and later fragments start
without a name or message. Here the code fragment kept `@qf-line`; the labels
were then applied explicitly. The next step moves to durable names for the
segments that will be mapped.

## 6. Label a segment

A label is covering-local status metadata — *not* part of the segment
hash, so relabelling leaves the hash unchanged and never breaks a link.
Use labels for coarse status: `unknown`, `reviewed`, `mapped`, `ignored`.

```sh
covmap label main:queue.md:14 reviewed     # the Size segment covers line 14
```

## 7. Name the segments you intend to map

A **name** gives a segment a stable, covering-local identity (a
*segment*), independent of its bytes. This is the key to drift-resistant
links: a connection that references `@name` survives edits to the segment's
content. Name the segments you are about to map.

```sh
covmap name main:queue.md:5     enqueue-op        # Enqueue
covmap name main:queue.md:10    dequeue-op        # Dequeue
covmap name main:errors.md:3:17 queuefull-meaning # the meaning clause (col 17+)
covmap name main:@qf-line -                       # drop the temporary working name
covmap ls -l
```

```
62ba072cd07be785	head	            errors.md:1..2	    2	-
d34ef5055873d546	QueueFull-code	    errors.md:3..3:16	1	-
934ef24fe0e3628e	QueueFull-meaning	errors.md:3:17..4	2	@queuefull-meaning
79c6668c1fcd99cd	preamble	        queue.md:1..4	    4	-
5eaec7059e0d1be3	Enqueue	            queue.md:5..9	    5	@enqueue-op
1287cbcf2aa0a3b2	Dequeue	            queue.md:10..13	    4	@dequeue-op
d3ed903c33afcf9d	reviewed	        queue.md:14..16	    3	-
```

The name is *not* part of the segment hash — naming, relabelling, or
recutting a segment never touches the shared object store.

## 8. Annotate a segment with why it was cut

A **message** records free-form rationale *about a segment itself* — why
this boundary exists, what the chunk is, why it is (or isn't) mapped. It is
covering-local and, like a name, **not** part of the segment hash, so
editing it never causes drift and it travels with the segment across later
edits. `annotate -m` is repeatable — each `-m` is a paragraph, git-style.

```sh
covmap annotate main:@enqueue-op \
  -m "Core enqueue op — mapped to the Rust impl." \
  -m "Cut as its own segment so the spec<->impl link is 1:1."
covmap annotate main:@enqueue-op    # no -m: print the message
covmap show main:@enqueue-op        # metadata, links, and numbered source text
```

`covmap ls -l` shows the message's first line as a trailing column; `show`
prints the full message with the segment header and includes the Enqueue text
with source line numbers beneath it. Use `--no-content` for metadata only.

This is the fifth and last place text lives in covmap — keep the five
straight by *what the text is about*:

| Channel | About | Hashed? |
| ------- | ----- | ------- |
| **label** | a coarse status category of a segment | no |
| **name** | a stable slug identity of a segment | no |
| **message** | why *this segment* (rationale/provenance) | no |
| link note | why *this mapping* (A↔B) | n/a |
| iterator task note | *this sweep's* verdict on a segment | n/a |

All three message-bearing operations use the same explicit-input convention:
repeat `-m`/`--message` for ordered paragraphs. A single value may also
contain line breaks. One invocation replaces the complete stored text and
`-m ""` clears it. Absence remains operation-specific: annotate reads, a
re-link preserves, and iterator resolution clears.

## 9. Connect, repair a rename, and disconnect

A **connection** records links between segments of two coverings. Create
it, then add links. When you link a *named* segment, covmap stores the link
by `@name` (resilient), not by hash.

These two commands do different jobs:

- `covmap peer bind <alias> <checkout-path>` teaches this checkout where a
  peer repository currently lives. Here `impl` is the durable alias and
  `../impl_rs` is its machine-local path. This does not create a connection.
- `covmap connect <source-covering> <alias>:<target-covering>
  <connection-name>` creates the portable relation. Here the source covering
  is the spec's `main`, `impl:main` means covering `main` in peer `impl`, and
  `queue_links` names the new connection.

The connection stores `impl`, not `../impl_rs`, so another checkout can bind
the same alias to a different path. The implementation side is currently one
whole-file segment, addressed as `queue.rs:1`. Because each spec endpoint is
named, the links below are stored by `@name` and survive later content edits.

```sh
covmap peer bind impl ../impl_rs             # alias -> local checkout path
covmap connect main impl:main queue_links    # source -> peer:target, named relation
covmap link queue_links @enqueue-op        queue.rs:1 -m "fn enqueue"
covmap link queue_links @dequeue-op        queue.rs:1 \
  -m "The dequeue operation delegates empty-queue handling to the same implementation segment while preserving the mapping rationale for review." \
  -m "Validated against the empty-queue branch in queue.rs."
covmap link queue_links @queuefull-meaning queue.rs:1 -m "QueueFull guard"
covmap ls-links queue_links
```

On an existing pair, a bare re-link preserves the note and `-m ""` clears it.
`ls-links` keeps one physical line per link by escaping backslash, TAB, CR,
and LF in the note field.

```
@dequeue-op	b5368af9628e9079	The dequeue operation delegates empty-queue handling to the same implementation segment while preserving the mapping rationale for review.\n\nValidated against the empty-queue branch in queue.rs.
@enqueue-op	b5368af9628e9079	fn enqueue
@queuefull-meaning	b5368af9628e9079	QueueFull guard
```

(All three map to the single whole-file impl segment here; in a real
project you would refine the impl side too.)

The target may also be a covering in the current repository. In
`connect main main local_preview`, the first `main` is the source covering,
the second `main` is the target covering, and `local_preview` is the
connection name. There is no `<peer>:` prefix because both sides are local.
Since both covering names are also the same, `status` and `show` mark this
relation as a self-map. It starts with no links, and `disconnect` removes only
its connection ref:

```sh
covmap connect main main local_preview   # source covering, target covering, name
covmap show local_preview
# connection local_preview   main -> main (self-map)
#   0 links · source 0/7 segments · target 0/7 segments
#   degree max-out 0, max-in 0
covmap disconnect local_preview
# disconnected 'local_preview': removed 0 link(s) from main -> main (self-map)
```

Now return to the external connection created at the start of this section.
Step 7 named the error-clause segment `@queuefull-meaning`, and the
`queue_links` connection stored that name as one of its source endpoints.
Names are covering-local, so renaming it does not silently rewrite the
connection. Instead, the command identifies the now-broken stored endpoint
and prints an explicit repair:

```sh
covmap name main:@queuefull-meaning queuefull-clause
```

```text
renamed @queuefull-meaning -> @queuefull-clause in covering 'main'
connection 'queue_links': stored source endpoint '@queuefull-meaning' is now broken
--- covmap repair begin ---
covmap link 'queue_links' '@queuefull-clause' 'b5368af9628e9079' -m 'QueueFull guard'
covmap unlink 'queue_links' '@queuefull-meaning' 'b5368af9628e9079'
--- covmap repair end ---
note: inbound connections stored in another repository cannot be discovered here
```

Execute the commands between the delimiters. They add the replacement before
removing the stale pair, so a failed `link` leaves the old record available,
and they preserve the existing note byte-for-byte. The tutorial script runs
this block and asserts that `QueueFull guard` is unchanged. If covmap reports
a broken far endpoint or a replacement collision, it emits no repair block;
do not invent one. A fan-out rename still emits one block, with every safe
`link` before any `unlink`.

## 10. Inspect: show, grep, diff, fsck, status

```sh
covmap show queue_links     # connection metadata + links
covmap grep capacity        # segments whose label or content matches
covmap cover alt            # create alt now, using the repository include
covmap status alt           # verify that the new covering exists
covmap diff main alt        # only-in-main (-) / only-in-alt (+) / shared (=)
covmap fsck                 # verify policy, partitions, and endpoints
covmap status
```

`cover alt` creates a new covering. It uses the same repository include as
`main`, but it has not been refined: `queue.md` and `errors.md` each remain one
whole-file segment. The diff therefore demonstrates a useful fact: coverings
can partition the same selected sources differently. `fsck: ok` confirms every
covering is a valid partition and every connection endpoint resolves.
`status` shows the connection's **coverage**: how much of each side is mapped.

```
covering main: segments=7 lines=21
  drift: clean
connections:
  queue_links: main -> impl:main : 3 links, 0 broken · coverage source 3/7, target 1/1
```

## 10b. Bounded drill-down: understand a region

The point of inspection is *understanding*: seeing a piece of the spec
**together with** the implementation it maps to. You don't dump whole
coverings and grep — you drill down in a few calls, ending on a view that
resolves the relation for you.

```sh
covmap ls -l main queue.md errors.md # narrow: union of two files
covmap show main:queue.md            # decomposition: content + mapped image
covmap show main:queue.md:8..11      # both intersecting segments, in full
covmap show --label reviewed main    # label lens: concern across files
covmap show queue_links @enqueue-op <impl-hash> # both endpoint contents
covmap show queue_links              # degree hubs + readable notes
```

`ls` accepts several file or `file:a..b` regions, unions them without
duplicates, and prints the result in covering order. `show <C>:<file>` (add
`:a..b` for a line range) prints each selected source segment's full recorded
content with its resolved counterpart(s), so one call answers both "what is
this file made of?" and "how does this map?":

```
region main queue.md · 4 segment(s)
  @enqueue-op  Enqueue  queue.md:5..9
    -> unknown   queue.rs:1..15
       fn enqueue
--- content @enqueue-op ---
5 | ## Enqueue
...
  ...
```

A range includes every segment it intersects, in full, even when the query
starts or ends inside that segment. Region views do not recursively print
mapped target content; inspect one exact link with `covmap show <K> <A> <B>`
for role-qualified source and target content blocks. Use `--no-content` when
you want the compact relationship-only form.

The connection view reports resolved degree and bounded hubs. Here all three
source segments map to the same implementation segment:

```text
degree max-out 1, max-in 3
fan-in:
     3 <- unknown queue.rs:1..15
```

Degree uses resolved content-hash identity: repeated covering locations with
identical content form one node and one hub row.

Long link notes are printed on wrapped, indented lines below the resolved
endpoint pair. `ls-links --resolve` remains the unwrapped TAB-delimited
plumbing form.

`covmap show --label <l>` is the same view selected by label instead of
file — the way to pull a cross-cutting concern (spread over many files) into
one compact call; add `--content` only when the whole label-selected set is
intended. `covmap show main:@enqueue-op` on a single segment shows its content,
links in both directions, and drift state, naming the fix command if it has
drifted.

## 11. Iterators: work the covering as a worklist

A covering says *what* the pieces are. An **iterator** freezes it into an
*ordered, stateful worklist*: each segment becomes a **task** you pull one
at a time, work against, and mark `done` or `skipped`. The cursor and
per-task state live on disk (`.covmap/iterators/<name>`), so the plan is
durable and resumable — after a crash or a context compaction, `covmap iter
review current` re-establishes exactly where you were. It is a **snapshot**
layered *over* the covering: it never mutates a segment, so we run it here,
on the clean refined covering, before the drift edits below.

`iter new` first ensures every selected task has recorded bytes. On an empty
cache it reads each selected source file once and reconstructs only ranges that
still match their covering hashes. If a selected range has drifted, iterator
creation fails with restore-and-hydrate guidance and leaves no partial
worklist.

Freeze `main` in reading order, then check the tally:

```sh
covmap iter review new main --order file
covmap iter review status                # 0/7 resolved (0 done, 0 skipped) — 7 pending, no active task
```

The **canonical loop** pulls one task at a time. `next` hands you the first
pending task and is *refused* while the current task is still pending, so
you resolve each before advancing — no segment is stepped over. Keep the
loop body comment-free: an interactive **zsh** treats a pasted `#` as a
command, not a comment, which would leave the task unresolved and spin the
loop. Do the work between `next` and `done`:

```sh
until covmap iter review finished; do
  covmap iter review next
  covmap iter review done -m "audited" -m "validation performed"
done
covmap iter review status                # 7/7 resolved (7 done, 0 skipped) — finished
covmap iter review current               # reprint the cursor and its frozen bytes
```

Between `next` and `done`, inspect the frozen content and do the actual work.
`current` is the resume command after an interruption because it reprints both
the active task and the numbered content block you were reviewing. Use
`--no-content` when a script needs only the compact task row, or
`--content-plain` when raw bytes are more useful. Use `skip -m "why" -m
"remaining risk"` in place of `done` to pass deliberately.
Repeated values become paragraphs in one task note. Resolving without a
message clears any earlier task note; `peek --all` escapes its line breaks so
each task stays on one physical output line.

Each task starts with one parseable row,
`<@name|hash>\t<label>\t<file>:<a>..<b>`. `next` and `current` append the same
numbered content block as `show` by default. `peek` remains a compact
multi-task listing unless `--content` or `--content-plain` is explicit.

The enumeration guarantee is what makes "visit every segment exactly once"
a property the tool enforces: `next` advances to the first pending task
*globally* (not merely after the cursor), `peek` lists the pending tasks
you can `choose` among (a jump that leaves states untouched, so a later
`next` returns to anything still pending), and `finished` is true only when
nothing is pending.

Besides the loop, three verbs give you manual control. `choose <addr>`
jumps the cursor to any task by `@name` or hash prefix — out of order, e.g.
to handle a specific segment first. `skip -m "why"` resolves the current
task as a deliberate pass rather than `done`. `rewind [<addr>]` undoes a
resolution, returning a `done`/`skipped` task to pending. We show them on a
throwaway `plan` iterator, so the `review` sweep above stays untouched —
iterators are cheap, independent overlays:

```sh
covmap iter plan new main
covmap iter plan choose @enqueue-op
covmap iter plan skip -m "trivial — covered elsewhere"
covmap iter plan status                  # 1/7 resolved (0 done, 1 skipped) — 6 pending, no active task
covmap iter plan rewind @enqueue-op      # undo the skip
covmap iter plan rm                      # discard the scratch iterator
```

Because `choose` never changes a task's state and `rewind` can undo any
resolution, jumping around and skipping cannot strand a task — the covering
is always fully reachable.

An iterator can be **filtered to one label**: a backfill of just the
not-yet-refined `unknown` segments, for example. Here `alt` (created visibly
in step 10) still has one whole-file segment for each selected source, so the
filter freezes two tasks:

```sh
covmap iter backfill new alt --label unknown   # froze ... 2 task(s), label=unknown
covmap iter                                    # bare `iter` lists every iterator
```

Iterators are pure overlays, so removing them leaves the covering
untouched:

```sh
covmap iter review rm
covmap iter backfill rm
```

An iterator is *not* a drift tracker. `iter new` retains every frozen task's
recorded bytes, so the worklist remains stable; drift belongs to the live
covering and is repaired below with `recut`.

## 11b. Advanced: object cache and plumbing

Most workflows do not need to manage `.covmap/objects` directly. A covering
records each source range and its content hash; `.covmap/objects` is a local,
reconstructible cache of the corresponding bytes. If the cache is empty but
the checked-out source still matches the covering, normal commands can verify
and reconstruct what they need.

The companion script clears the cache to exercise that recovery path:

```sh
rm -rf .covmap/objects/*
covmap fsck --offline                 # verify refs without filling the cache
covmap rev-parse 238857a2             # resolve an abbreviated segment address
covmap cat-segment 238857a2           # print recorded bytes; cache if verified
covmap hydrate main                   # reconstruct all missing objects for main
covmap fsck --offline --hydrate       # verify and hydrate in one pass
```

`rev-parse` and `cat-segment` are plumbing commands, not prerequisites for
everyday use. `rev-parse` turns an address or unique hash prefix into the full
segment hash. `cat-segment` prints the immutable bytes recorded by that hash;
it can reconstruct them from source only when the source still verifies.
Prefer `show <address>` for human inspection and `hydrate` when you
need to repopulate a whole covering.

A missing object is not corruption when the current source reproduces its
recorded hash. Drifted source is never cached under the old hash, and a
present object with the wrong bytes is corruption. `hydrate` changes only the
local cache; it does not change a covering, repair drift, or overwrite a
corrupt object.

---

# Drift: choose the repair by what moved

A covering records both segment bytes and segment boundaries. Keep these
operations distinct:

- `split` defines a new boundary in a clean covering.
- `recut <segment>` accepts new bytes inside an existing boundary.
- `recut --remap` moves line-aligned boundaries through insertions and
  deletions when the segment order is unchanged.

Start with `covmap status`, then choose by the shape of the edit:

| Edit shape | Repair |
| ---------- | ------ |
| Bytes changed, boundary still correct | `recut` the affected segment |
| Lines inserted or deleted, segment order unchanged | `recut --remap` the file or covering |
| A segment moved, was reordered, or has character-level boundaries | Verify the new range, then rebuild the affected local partition manually |

The next three steps demonstrate the first two repairs and the diagnostic
support covmap provides for the harder third case.

## 12. In-place edit → `recut`; the named link survives

Tighten the Dequeue wording — *"…or None when empty."* becomes *"…or
None when the queue is empty."* (stage `files/queue.reworded.md`). The
edit changes bytes **without moving any line boundary** — segment
drift.

```sh
cp ../../files/queue.reworded.md queue.md    # the in-place wording edit
covmap status
```

```
  drift: 1 file(s) edited, 0 file(s) missing/binary
    edited: queue.md
  queue_links: main -> …/main : 3 links, 0 broken
```

Note the connection is **already 0 broken**: the link is by
`@dequeue-op`, and a byte change does not delete the segment. Refresh the
recorded content with a single `recut`:

```sh
covmap recut main:@dequeue-op          # address the segment by its name
covmap status main                     # main is clean, still 3 links, 0 broken
```

No unlink/relink is needed. An anonymous, hash-based link would still point to
the old hash after the recut, so it would become broken and require explicit
replacement.

## 13. Inserted lines → `recut --remap` (diff-remap)

Add two lines to the preamble (stage `files/queue.inserted.md`). Now
every section below shifts down — **boundary drift**. A per-segment
`recut` is the wrong tool (it would capture the wrong lines). Instead,
diff-remap:

```sh
cp ../../files/queue.inserted.md queue.md    # insert two preamble lines
covmap recut --remap
```

```
queue.md: remapped 4 segment(s) (1 refreshed, 3 unchanged)
remap: 1 file(s) healed, 1 segment(s) refreshed, 0 unresolved
```

It reconstructs the pre-edit file from the covering, diffs it against
disk, and re-fits every line-aligned segment's boundaries through the
unchanged regions — following the insertion automatically. Names (hence
the `queue_links` links) are preserved, so `status main` is clean again and
the connection is still 3 links, 0 broken. No relinking.

## 14. Moved section → fingerprint proposal (propose-only)

Move the whole **Enqueue** section to the end of the file (stage
`files/queue.moved.md`). Its boundary lines vanish from their old
position and reappear later, which breaks the monotonic line map —
diff-remap **cannot** place it. Instead it leaves the file untouched and
prints a ranked, scored relocation guess:

```sh
cp ../../files/queue.moved.md queue.md       # move Enqueue to the end
covmap recut --remap
```

```
queue.md: 1 segment boundary(ies) fall in changed regions — left untouched.
Fingerprint proposals (confirm manually; nothing was changed):
  @enqueue-op: queue.md:14..18  (98% match)  — covmap unlink/link to relocate
remap: 0 file(s) healed, 0 segment(s) refreshed, 1 unresolved
```

(The proposed window `14..18` includes the blank line before the moved
heading — a fuzzy guess, hence 98% rather than 100%. The other segments
re-fit cleanly, so only the moved one is unresolved.)

Fingerprinting is **propose-only**: a similarity score is a likelihood,
not a proof, so covmap never relinks on a fuzzy match by itself. You
must repair the covering geometry; changing a link alone cannot move a
segment. Tune which guesses appear with `--threshold <f>` (default 0.4).

For an unresolved move, use this sequence:

1. Stop after the proposal. The file's covering was left unchanged; do not
   `prune` temporarily broken links.
2. Compare the recorded segment (`covmap show main:@enqueue-op`)
   with the proposed source lines. Treat the score only as a pointer.
3. Record the affected segments' names, labels, messages, and link notes.
   Merge the smallest adjacent span that contains the old and new boundaries,
   recut that span from the verified file, then split it at every new boundary.
4. Reapply the recorded metadata to the correct new segments. Recreating the
   same names makes name-based links resolve again; only anonymous hash-based
   links require unlinking and relinking.
5. Run `covmap status`, `covmap fsck`, and `covmap show queue_links`. Finish
   only when drift is clean, no endpoint is broken, and the resolved mapping
   shows the intended source text.

## 15. Drop a genuinely broken link: `prune`

A link is *broken* only when its endpoint no longer resolves — for a
named segment, when that segment is deleted or its covering-local name is
cleared or changed. Clear a segment's name to simulate that, then prune:

```sh
covmap name main:@queuefull-clause -             # clears the name
covmap fsck --broken-links                       # queue_links  @queuefull-clause  …
covmap prune queue_links                         # drops the dead link
```

```
broken: @queuefull-clause (missing in source covering) -> … (ok): dropped
prune: 1 link(s) dropped, 2 kept
```

## 16. Render the HTML report

```sh
covmap html
# wrote .covmap/report.html
```

Open `.covmap/report.html` (i.e. `tutorial/_play/spec/.covmap/report.html`
if you followed along by hand, or `tutorial/_work/spec/.covmap/report.html`
when the script is run with `KEEP=1`) in a browser for the List / Source /
Target / Compare views, with character-level chunks rendered inline and linked
segments highlighted.

In the Compare view, clicking a segment highlights all its counterparts on
the other side; clicking the *same* segment again cycles through them one
at a time, scrolling each into view (handy when one item maps to many).
Hovering a segment shows a tooltip *about that segment* — its location,
label, comment, and the links it participates in (for a target segment,
the source segments pointing back at it). The link rows are informational;
navigation across sides is by clicking the segment in the panel. The small
`↗` in the tooltip header opens **that same segment** in its own view — a
source segment in the Source view, a target segment in the Target view.
When you follow a `↗`, the landed segment keeps a blue ring marking where
you came from, and a floating **↩ Back to Compare** button returns you to
it. The Source and Target views also carry a `⇄` on each link that jumps
into Compare and lands on the link directly.

The report shows one connection at a time, so its Target / Compare views
render a single coherent target covering. When exactly one connection
exists it is selected automatically; with several, `covmap html` lists them
and asks you to pick one with `--connection <K>`. To point the report at a
specific map regardless of HEAD or your cwd, pass `--covering <C>` and/or
`--connection <K>` — e.g. `covmap html --connection queue_links` fixes the
source to that connection's source covering and renders its target side in
the Target / Compare views. With no connections yet, the report still shows
the List / Source views of HEAD's covering.

---

## What this covered

| Feature | Step |
| ------- | ---- |
| init / cover / ignore | 1 |
| status, ls, cache verification / hydration | 2, 10, 11b |
| atomic multi-cut (line + character) | 3, 5 |
| merge / recut / label | 4, 6, 12 |
| names (segments) | 7 |
| annotate (segment message) | 8 |
| connect / link / addressing / rename repair / disconnect | 9 |
| show / grep / diff / fsck | 10 |
| iterator: freeze + sweep a worklist | 11 |
| drift: recut (named link survives) | 12 |
| drift: diff-remap | 13 |
| drift: fingerprint proposal | 14 |
| prune | 15 |
| HTML report | 16 |
