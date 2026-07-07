# covmap tutorial

A guided, hands-on tour of every main covmap feature, end to end. It
maps a small Markdown **spec** to a Rust **implementation**, then edits
the spec three different ways to exercise covmap's drift repair — the
names, diff-remap, and fingerprinting layers.

Everything here is executed by the companion script
[`run_tutorial.sh`](run_tutorial.sh), which is the source of truth: this
document narrates exactly the commands it runs, and the script asserts
the key outcomes so it doubles as an integration test.

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

First make sure the `covmap` you'll type is the **current** build. The
positional addresses used below (`main:queue.md:5`) are a recent
feature, so a stale binary on your `PATH` will reject them with
`no segment matches 'queue.md:5'`. Build and use the fresh one:

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
| initial | `files/queue.md` | steps 1–9 |
| in-place edit | `files/queue.reworded.md` | step 10 |
| inserted lines | `files/queue.inserted.md` | step 11 |
| moved section | `files/queue.moved.md` | step 12 |

(Stage files are named for the *edit* they represent, not a step number,
so they stay meaningful if the tutorial is reordered.)

At each drift step, "edit the file" means **copy the next stage over
`queue.md`** — e.g. `cp ../../files/queue.reworded.md queue.md` (run
from `_play/spec`, where you are). That is exactly what the script does,
so your hashes and line numbers will match this document. When you're
done, `rm -rf tutorial/_play` to clean up.

This tutorial touches every requirement in
[`../REQUIREMENTS.md`](../REQUIREMENTS.md) except the connection
*algebra* (inverse / composition / union, req11), which is part of the
formal model but has no CLI surface yet. Each step is annotated with the
requirements it exercises. For the agent-facing reference, see
[`../SKILL.md`](../SKILL.md); for fixture-based evaluation tasks, see
[`../skill_playground/`](../skill_playground/README.md) (this tour folds
in the scenarios from tasks T1–T8).

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

## 1. Initialize, ignore, cover  (req1, req6, req7, req18)

Each repository gets its own `.covmap/`. **Before the first `cover`**,
write `.covmap/ignore` so generated files never enter the covering — the
single most important setup step.

```sh
covmap init                          # in _play/spec
(cd ../impl_rs && covmap init)

printf 'build/\n' > .covmap/ignore   # keep build output out

covmap cover
(cd ../impl_rs && covmap cover)
```

`cover` adds one whole-file `unknown` segment per readable file it has
not yet covered — this first, unrefined covering is the *initial cover*.
It is additive and idempotent. `build/artifact.txt` is absent from the
covering because `build/` is ignored.

```
Covering 'main': 2 segments, 20 lines (2 added)
```

## 2. Inspect the covering  (req12, req20)

```sh
covmap status
covmap ls -l
```

```
covering main: segments=2 lines=20
  drift: clean
327ef5a116484a87	unknown	errors.md:1..4	4	-
9ce81ba012a36b2a	unknown	queue.md:1..16	16	-
```

`ls -l` columns are: hash, label, location (`file:a..b`), line count, and
name (`-` when anonymous). `status` reports cardinality, line count, and
drift.

## 3. Refine into one segment per section  (req5, req8)

There are no automatic segmentation rules — you refine by hand with
`split`. You point at the segment to cut by **position**, not by hash:
`main:queue.md:5` means *"the segment of `queue.md` that covers line 5"*.
Exactly one segment covers any given line, so this address is
unambiguous — and, unlike a hash, it never changes if the content or the
hashing does. (You can still use a hash prefix or an `@name`; position
is just the friendliest handle while you're reading the file.)

The `##` headings sit at lines 5, 10, 14 (the Enqueue body wraps onto
two lines, so the sections are *not* uniformly sized — read the line
numbers off the file, don't assume them). `split`'s second argument is
the line to cut at; the top keeps the first label, the bottom gets the
second:

```sh
covmap split main:queue.md:1  5 preamble Enqueue   # cut the whole file at line 5
covmap split main:queue.md:5  10 Enqueue Dequeue   # cut the 5.. segment at line 10
covmap split main:queue.md:10 14 Dequeue Size      # cut the 10.. segment at line 14
covmap ls -l
```

Each address names a *current* segment (`:1` the whole file, then `:5`
and `:10` the growing bottom piece), and the cut line follows. Result:

```
preamble	queue.md:1..4	4	-
Enqueue	queue.md:5..9	5	-
Dequeue	queue.md:10..13	4	-
Size	queue.md:14..16	3	-
```

(Hashes omitted above for readability — yours will match a fresh run,
since identity is content-addressed, but you never need to type them.)

## 4. Refinement is reversible: merge, then re-split  (req8)

Merge the two adjacent segments (Enqueue covers line 5, Dequeue covers
line 10), then split the result — which now covers line 5 — back at
line 10:

```sh
covmap merge main:queue.md:5 main:queue.md:10 "Enq+Deq"
covmap split main:queue.md:5 10 Enqueue Dequeue
```

`merge` requires the two segments to be adjacent; `split` puts them back.
Because identity is content-addressed, splitting back to the same
boundaries with the same labels restores the original segment hashes
exactly.

## 5. Character-level split: isolate a clause inside a line  (req5)

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
covmap split main:errors.md:3 3 head qf-line   # the QueueFull line is now its own segment
covmap name  main:errors.md:3 qf-line          # give it a stable name
covmap split main:@qf-line 3:17 QueueFull-code QueueFull-meaning
covmap ls -l
```

The cut argument carries the column too: `3:17` cuts at character 17 of
line 3 — where "the queue is at capacity" begins.

```
head	          errors.md:1..2	    2	-
QueueFull-code	  errors.md:3..3:16	1	@qf-line
QueueFull-meaning errors.md:3:17..4	2	-
```

Two things to notice. The locations `errors.md:3..3:16` and
`errors.md:3:17..4` are character-precise — the cut is between columns
16 and 17 of line 3. And on a split **the name stays with the top
half** (`QueueFull-code` kept `@qf-line`); the bottom is anonymous. If
you wanted the name on the meaning clause instead, just re-`name` it —
which is exactly what the next step does with proper names.

## 6. Label a segment  (req4)

A label is part of a segment's content identity (relabelling re-hashes).
Use labels for coarse status: `unknown`, `reviewed`, `mapped`, `ignored`.

```sh
covmap label main:queue.md:14 reviewed     # the Size segment covers line 14
```

## 7. Name the segments you intend to map  (req3, req8)

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
6fefd6238c5bb0e3	head	            errors.md:1..2	    2	-
4a68e8bf8cbc096f	QueueFull-code	    errors.md:3..3:16	1	-
3b6f97be0726dc24	QueueFull-meaning	errors.md:3:17..4	2	@queuefull-meaning
89c0950692f71928	preamble	        queue.md:1..4	    4	-
f6d8bf2f060c7830	Enqueue	            queue.md:5..9	    5	@enqueue-op
2e57336943c86e53	Dequeue	            queue.md:10..13	    4	@dequeue-op
a51b8db403cc3096	reviewed	        queue.md:14..16	    3	-
```

The name is *not* part of the segment hash — naming, relabelling, or
recutting a segment never touches the shared object store.

## 8. Connect and link by name  (req2, req10, req16)

A **connection** records links between segments of two coverings. Create
it, then add links. When you link a *named* segment, covmap stores the link
by `@name` (resilient), not by hash.

`connect` names the target repo by absolute path (here the sibling
`impl_rs`). The impl side is a single whole-file segment, which we
address positionally as `queue.rs:1`. Link each named spec segment to it:
because the spec side is named, the link is stored by `@name` and
survives later edits.

```sh
covmap connect main "$PWD/../impl_rs:main" queue_links
covmap link queue_links @enqueue-op        queue.rs:1 "fn enqueue"
covmap link queue_links @dequeue-op        queue.rs:1 "fn dequeue"
covmap link queue_links @queuefull-meaning queue.rs:1 "QueueFull guard"
covmap ls-links queue_links
```

```
@enqueue-op	75db7ddb38ccc701	fn enqueue
@dequeue-op	75db7ddb38ccc701	fn dequeue
@queuefull-meaning	75db7ddb38ccc701	QueueFull guard
```

(All three map to the single whole-file impl segment here; in a real
project you would refine the impl side too.)

## 9. Inspect: show, grep, diff, fsck, status  (req15, req19, req20)

```sh
covmap show queue_links     # connection metadata + links
covmap grep capacity        # segments whose label or content matches
covmap cover alt            # a second, whole-file covering
covmap diff main alt        # added (-) / only-in-alt (+) / shared (=)
covmap fsck                 # verify partitions + connection endpoints
covmap status
```

`fsck: ok` confirms every covering is a valid partition and every
connection endpoint resolves. `status` now shows the connection:

```
covering main: segments=7 lines=21
  drift: clean
connections:
  queue_links -> <workspace>/impl_rs/main : 3 links, 0 broken
```

## 10. Iterators: work the covering as a worklist  (req21)

A covering says *what* the pieces are. An **iterator** freezes it into an
*ordered, stateful worklist*: each segment becomes a **task** you pull one
at a time, work against, and mark `done` or `skipped`. The cursor and
per-task state live on disk (`.covmap/iterators/<name>`), so the plan is
durable and resumable — after a crash or a context compaction, `covmap iter
review current` re-establishes exactly where you were. It is a **snapshot**
layered *over* the covering: it never mutates a segment, so we run it here,
on the clean refined covering, before the drift edits below.

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
  covmap iter review done -m "audited"
done
covmap iter review status                # 7/7 resolved (7 done, 0 skipped) — finished
covmap iter review current               # the resume affordance: reprint the cursor's task
```

(Between `next` and `done` you would inspect the task — `covmap iter review
peek --content`, or `covmap cat-segment <hash>` — and do the actual work;
use `skip -m "why"` in place of `done` to pass on one deliberately.)

Each `next` / `current` / `peek` prints one parseable line per task —
`<@name|hash>\t<label>\t<file>:<a>..<b>` — and `--content` additionally
dumps the frozen bytes, so the loop's stdout stays terse by default.

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

An iterator can be **filtered to one label** — a backfill of just the
not-yet-refined `unknown` segments, say. Here `alt` (from step 9) is the
whole-file covering, so this freezes its two `unknown` segments:

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

(This is playground task T8. Note an iterator is *not* a drift tracker: a
frozen task never goes stale — its content is pinned by hash — so drift
stays a property of the covering, repaired below with `recut`.)

---

# Drift: three edits, three repair layers

A covering exists to be reused after the source is edited. covmap detects
when files diverge from their recorded segments (**drift**) and offers a
layered repair path. The next three steps each make a different kind of
edit.

## 11. In-place edit → `recut`; the named link survives  (req13, req14)

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
  queue_links -> …/main : 3 links, 0 broken
```

Note the connection is **already 0 broken**: the link is by
`@dequeue-op`, and a byte change does not delete the segment. Refresh the
recorded content with a single `recut`:

```sh
covmap recut main:@dequeue-op          # address the segment by its name
covmap status                          # drift: clean, still 3 links, 0 broken
```

No unlink/relink needed — contrast this with a hash-based link, which
would have broken (playground task T3 vs. T5).

## 12. Inserted lines → `recut --remap` (diff-remap)  (req13)

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
the `queue_links` links) are preserved, so `status` is clean again and
the connection is still 3 links, 0 broken. No relinking.

## 13. Moved section → fingerprint proposal (propose-only)

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
confirm the candidate (read those lines), then re-cut the segment at its new
home and re-link. Tune which guesses appear with `--threshold <f>`
(default 0.4). This is playground task T7.

The three repair layers form a chain: **name** keeps a link *alive*
across content edits → **diff-remap** deterministically re-fits
boundaries that survived in place → **fingerprint** proposes a relocation
for whatever remains.

## 14. Drop a genuinely broken link: `prune`  (req14)

A link is *broken* only when its endpoint no longer resolves — for a
named segment, when that segment is **deleted**. Clear a segment's name to
simulate that, then prune:

```sh
covmap name main:@queuefull-meaning -            # clears the name
covmap fsck --broken-links                       # queue_links  @queuefull-meaning  …
covmap prune queue_links                         # drops the dead link
```

```
broken: @queuefull-meaning (missing in source covering) -> … (ok): dropped
prune: 1 link(s) dropped, 2 kept
```

## 15. Render the HTML report  (req20)

```sh
covmap html
# wrote .covmap/report.html
```

Open `.covmap/report.html` (i.e. `tutorial/_play/spec/.covmap/report.html`
if you followed along by hand, or `tutorial/_work/spec/.covmap/report.html`
from the script) in a browser for the List / Source / Target / Compare
views, with character-level chunks rendered inline and linked segments
highlighted.

---

## What this covered

| Feature | Step | Requirement |
| ------- | ---- | ----------- |
| init / cover / ignore | 1 | req1, req6, req7, req18 |
| status, ls (metrics) | 2, 9 | req12, req20 |
| split (line + character) | 3, 5 | req5, req8 |
| merge / recut / label | 4, 6, 11 | req8, req4, req13 |
| names (segments) | 7 | req3, req8 |
| connect / link / addressing | 8 | req2, req10, req16 |
| show / grep / diff / fsck | 9 | req15, req19, req20 |
| iterator: freeze + sweep a worklist | 10 | req21 |
| drift: recut (named link survives) | 11 | req13, req14 |
| drift: diff-remap | 12 | req13 |
| drift: fingerprint proposal | 13 | req13, req14 |
| prune | 14 | req14 |
| HTML report | 15 | req20 |

Not shown: connection algebra (inverse / composition / union, req11),
which is defined in the model but not yet exposed on the CLI.
