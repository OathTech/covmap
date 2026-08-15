#!/usr/bin/env bash
#
# run_tutorial.sh — the executable companion to TUTORIAL.md.
#
# It builds a throwaway workspace under tutorial/_work/ holding a tiny
# Markdown spec and a Rust implementation, then runs every command the
# tutorial walks through, in order. The script is the source of truth:
# TUTORIAL.md narrates exactly these steps.
#
# Usage:
#   ./run_tutorial.sh          # run the whole tutorial in a fresh _work/
#   KEEP=1 ./run_tutorial.sh   # keep _work/ afterwards (for inspection / html)
#   ./run_tutorial.sh clean    # remove _work/ and exit
#
# It is self-contained and idempotent: every run wipes and rebuilds
# _work/ first, so you can re-run it freely.

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
WORK="$HERE/_work"

# ---- locate / build the binary --------------------------------------------
COVMAP="${COVMAP_BIN:-$ROOT/target/release/covmap}"
if [[ ! -x "$COVMAP" ]]; then
  echo "==> building covmap (release)"
  ( cd "$ROOT" && cargo build --release >/dev/null )
fi

if [[ "${1:-}" == "clean" ]]; then
  rm -rf "$WORK"
  echo "removed $WORK"
  exit 0
fi

# Pretty section headers + a `run` helper that echoes each command.
hr()  { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }
run() { printf '\033[2m$ covmap %s\033[0m\n' "$*"; "$COVMAP" "$@"; }

# Assertion helper so the tutorial doubles as an integration check.
assert() { # <description> <command...>
  local msg="$1"; shift
  if "$@" >/dev/null 2>&1; then printf '  \033[32mOK\033[0m  %s\n' "$msg"
  else printf '  \033[31mFAIL\033[0m  %s\n' "$msg"; exit 1; fi
}

run_repair_block() { # <cwd> <captured name output>
  local cwd="$1" output="$2" script
  script=$(printf '%s\n' "$output" \
    | sed -n '/^--- covmap repair begin ---$/,/^--- covmap repair end ---$/p' \
    | sed '1d;$d')
  if [[ -z "$script" ]]; then
    printf '  \033[31mFAIL\033[0m  rename emitted no repair block\n'
    exit 1
  fi
  printf '%s\n' "$script" | sed 's/^/$ /'
  ( cd "$cwd" && PATH="$(dirname "$COVMAP"):$PATH" sh -eu -c "$script" )
}

# ---- 0. fresh workspace ----------------------------------------------------
# The spec/impl source files are kept as fixtures under tutorial/files/,
# so the tutorial never edits a file "in place" with an opaque sed/perl
# program. The drift steps simply `cp` the next staged version of
# queue.md into place — and you can reproduce any stage by hand with the
# same copy. Stages are named for the EDIT they represent (not a step
# number), so they stay meaningful if steps are reordered:
#   files/queue.md            initial spec
#   files/queue.reworded.md   after the in-place wording edit
#   files/queue.inserted.md   after inserting two preamble lines
#   files/queue.moved.md      after moving the Enqueue section
#   files/errors.md, files/queue.rs   the other two sources
FILES="$HERE/files"

rm -rf "$WORK"
mkdir -p "$WORK/spec" "$WORK/impl_rs"

# Stage the initial source files.
cp "$FILES/queue.md"  "$WORK/spec/queue.md"
cp "$FILES/errors.md" "$WORK/spec/errors.md"
cp "$FILES/queue.rs"  "$WORK/impl_rs/queue.rs"

# Build output we do NOT want covered — demonstrates `.covmap/ignore`.
mkdir -p "$WORK/spec/build"
echo "generated junk" > "$WORK/spec/build/artifact.txt"

cat <<EOF

This tutorial maps a Markdown spec (spec/) to a Rust implementation
(impl_rs/), then edits the spec three ways to exercise covmap's drift
repair. Workspace: $WORK
EOF

#############################################################################
hr "1. init + ignore + cover  (REQ1, REQ6, REQ7, REQ18)"
#############################################################################
( cd "$WORK/spec"    && run init )
( cd "$WORK/impl_rs" && run init )

# Keep build output out of the covering BEFORE the first cover.
printf 'build/\n' > "$WORK/spec/.covmap/ignore"
echo "  (wrote spec/.covmap/ignore: build/)"

( cd "$WORK/spec"    && run cover )
( cd "$WORK/impl_rs" && run cover )

assert "build/ was ignored (not covered)" \
  bash -c "! ( cd '$WORK/spec' && '$COVMAP' ls main | grep -q 'build/artifact' )"

#############################################################################
hr "2. inspect the covering  (REQ12, REQ20)"
#############################################################################
( cd "$WORK/spec" && run status )
( cd "$WORK/spec" && run ls -l )

#############################################################################
hr "3. refine queue.md into one segment per section  (REQ5, REQ8)"
#############################################################################
# The `##` headings sit at lines 5, 10, 14 (the Enqueue body wraps onto
# two lines, so the sections are not uniform). A file selector applies every
# cut against the covering state present when the command starts and writes
# the covering ref once. Cuts may be unsorted; output is in file order.
( cd "$WORK/spec" && run split main:queue.md 14 5 10 )
# Split changes geometry only. Apply status metadata explicitly afterward.
( cd "$WORK/spec" && run label main:queue.md:1  preamble )
( cd "$WORK/spec" && run label main:queue.md:5  Enqueue )
( cd "$WORK/spec" && run label main:queue.md:10 Dequeue )
( cd "$WORK/spec" && run label main:queue.md:14 Size )
( cd "$WORK/spec" && run ls -l )

assert "queue.md now has >=4 segments" \
  bash -c "[[ \$( ( cd '$WORK/spec' && '$COVMAP' ls main | grep -c 'queue.md' ) ) -ge 4 ]]"

#############################################################################
hr "4. merge + re-split to show refinement is reversible  (REQ8)"
#############################################################################
# Address the two adjacent segments positionally: Enqueue covers line 5,
# Dequeue covers line 10. After the merge, the combined segment covers
# line 5; re-split it at line 10.
( cd "$WORK/spec" && run merge main:queue.md:5 main:queue.md:10 "Enq+Deq" )
( cd "$WORK/spec" && run split main:queue.md:5 10 )
( cd "$WORK/spec" && run label main:queue.md:5  Enqueue )
( cd "$WORK/spec" && run label main:queue.md:10 Dequeue )

#############################################################################
hr "5. character-level split: isolate a clause inside one line  (REQ5)"
#############################################################################
# errors.md line 3 is "QueueFull means the queue is at capacity ...". We
# isolate the meaning clause ("the queue is at capacity ...") as its own
# segment by splitting mid-line at the column where it begins. This keeps
# queue.md fully line-aligned for the drift demos below.
echo "  errors.md content:"
( cd "$WORK/spec" && "$COVMAP" cat-segment "$( "$COVMAP" ls main | grep -F 'errors.md' | cut -f1 )" | sed 's/^/    /' )
# Column where "the queue is at capacity" begins on line 3.
ECOL=$(awk 'NR==3{ print index($0,"the queue is at capacity") }' "$WORK/spec/errors.md")
echo "  clause begins at line 3, column $ECOL"
# Split the segment covering errors.md line 3 first at line 3 (so the
# QueueFull line is on its own). Name that line `qf-line`, then split it
# BY NAME at the clause column — a name is a unique, readable handle for
# the chunk you're working on. (On a split the name stays with the first
# fragment; step 7 clears this temporary name.)
( cd "$WORK/spec" && run split main:errors.md 3 )
( cd "$WORK/spec" && run label main:errors.md:1 head )
( cd "$WORK/spec" && run label main:errors.md:3 qf-line )
( cd "$WORK/spec" && run name  main:errors.md:3 qf-line )
( cd "$WORK/spec" && run split main:@qf-line "3:$ECOL" )
( cd "$WORK/spec" && run label main:@qf-line QueueFull-code )
( cd "$WORK/spec" && run label "main:errors.md:3:$ECOL" QueueFull-meaning )
( cd "$WORK/spec" && run ls -l )

assert "a character-level segment exists on errors.md line 3" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' ls -l main | grep -qE 'errors\.md:3:[0-9]+' )"

#############################################################################
hr "6. label a segment  (REQ4)"
#############################################################################
# Size is the segment covering queue.md line 14. A label is ref-local
# metadata, so changing it must preserve the content hash.
SIZE_BEFORE=$( cd "$WORK/spec" && "$COVMAP" rev-parse main:queue.md:14 )
( cd "$WORK/spec" && run label main:queue.md:14 reviewed )
SIZE_AFTER=$( cd "$WORK/spec" && "$COVMAP" rev-parse main:queue.md:14 )

assert "relabeling preserves the segment hash" \
  test "$SIZE_BEFORE" = "$SIZE_AFTER"

#############################################################################
hr "7. name the segments we intend to map  (REQ3, REQ8)"
#############################################################################
# Names give a segment a stable identity, so links survive edits. We
# name the two queue.md operations (at lines 5 and 10) and the errors.md
# meaning clause (line 3, after the mid-line cut, so column 17+).
( cd "$WORK/spec" && run name main:queue.md:5     enqueue-op )
( cd "$WORK/spec" && run name main:queue.md:10    dequeue-op )
( cd "$WORK/spec" && run name main:errors.md:3:17 queuefull-meaning )
( cd "$WORK/spec" && run name main:@qf-line - )   # drop the temporary working name
( cd "$WORK/spec" && run ls -l )

#############################################################################
hr "8. annotate a segment with why it was cut  (REQ22)"
#############################################################################
# A `message` records free-form rationale ABOUT a segment — why this
# boundary exists, what the chunk is — distinct from its label (a status
# category) and its name (a slug identity). Like the label and name it is
# ref-local and not part of the hash, so editing it never causes drift, and
# it travels with the segment across later edits. `annotate -m` is
# repeatable (paragraphs, git-style).
( cd "$WORK/spec" && run annotate main:@enqueue-op \
    -m "Core enqueue op — mapped to the Rust impl." \
    -m "Cut as its own segment so the spec<->impl link is 1:1." )
( cd "$WORK/spec" && run annotate main:@enqueue-op )   # no -m: print the message
( cd "$WORK/spec" && run show main:@enqueue-op | sed -n '1,6p' )

assert "the message is stored on the segment (shown by ls -l)" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' ls -l main | grep -qF 'Core enqueue op' )"

#############################################################################
hr "9. connect spec -> impl and link by name  (REQ2, REQ10, REQ16)"
#############################################################################
( cd "$WORK/spec" && run connect main "$WORK/impl_rs:main" queue_links )
IMPL=$( cd "$WORK/impl_rs" && "$COVMAP" ls-segments )
DEQUEUE_NOTE="The dequeue operation delegates empty-queue handling to the same implementation segment while preserving the mapping rationale for review."
( cd "$WORK/spec" && run link queue_links @enqueue-op        "$IMPL" "fn enqueue" )
( cd "$WORK/spec" && run link queue_links @dequeue-op        "$IMPL" "$DEQUEUE_NOTE" )
( cd "$WORK/spec" && run link queue_links @queuefull-meaning "$IMPL" "QueueFull guard" )
( cd "$WORK/spec" && run ls-links queue_links )

assert "connection has 3 links, all by @name" \
  bash -c "[[ \$( ( cd '$WORK/spec' && '$COVMAP' ls-links queue_links | grep -c '^@' ) ) -eq 3 ]]"

echo "  same-repository shorthand and lifecycle:"
( cd "$WORK/spec" && run connect main main local_preview )
( cd "$WORK/spec" && run show local_preview )
assert "status identifies the temporary connection as a self-map" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' status ) | grep -qF 'local_preview: main -> main (self-map)'"
( cd "$WORK/spec" && run disconnect local_preview )
assert "disconnect removed only the temporary connection ref" \
  test ! -e "$WORK/spec/.covmap/connections/local_preview"

echo "  covering-local rename and explicit connection repair:"
printf '\033[2m$ covmap name main:@queuefull-meaning queuefull-clause\033[0m\n'
RENAME_OUT=$( cd "$WORK/spec" && "$COVMAP" name main:@queuefull-meaning queuefull-clause )
printf '%s\n' "$RENAME_OUT"
run_repair_block "$WORK/spec" "$RENAME_OUT"

assert "rename repair preserved the link and its note" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' ls-links queue_links ) | grep -qF \$'@queuefull-clause\\t$IMPL\\tQueueFull guard'"
assert "rename repair restored connection integrity" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' fsck )"

#############################################################################
hr "10. inspect: show, grep, diff, fsck, status  (REQ15, REQ19, REQ20)"
#############################################################################
( cd "$WORK/spec" && run show queue_links )
( cd "$WORK/spec" && run grep capacity )
# diff two coverings: build a second whole-file covering `alt` and diff
# it against the refined `main`. Shared segments print with `=`, segments
# only in one side with `-` / `+`.
( cd "$WORK/spec" && run cover alt )
( cd "$WORK/spec" && run diff main alt )
( cd "$WORK/spec" && run fsck )
( cd "$WORK/spec" && run status )

assert "fsck passes after linking" bash -c "( cd '$WORK/spec' && '$COVMAP' fsck )"

#############################################################################
hr "10b. bounded drill-down: understand a region in a few calls  (REQ23)"
#############################################################################
# The inspection access pattern: narrow to a region, then `show` it to see
# each segment together with what it maps to — resolved to label+loc, in one
# self-sufficient call (no bare-hash hand-joining).
( cd "$WORK/spec" && run ls -l main queue.md errors.md ) # union of two files
( cd "$WORK/spec" && run show main:queue.md )            # region: segments + images
( cd "$WORK/spec" && run show --label reviewed main )    # concern across files
( cd "$WORK/spec" && run show queue_links )               # degree hubs + notes

assert "multi-region ls returns both files once in covering order" bash -c "
  out=\$(cd '$WORK/spec' && '$COVMAP' ls -l main errors.md queue.md:5..10 queue.md:10..10)
  [[ \$(printf '%s\n' \"\$out\" | wc -l | tr -d ' ') -eq 5 ]] \
    && [[ \$(printf '%s\n' \"\$out\" | grep -c 'errors.md') -eq 3 ]] \
    && [[ \$(printf '%s\n' \"\$out\" | grep -c 'queue.md') -eq 2 ]] \
    && ! printf '%s\n' \"\$out\" | grep -q 'queue.md:1..4'"

# `show <connection>` now resolves endpoints and reports coverage.
assert "show queue_links reports coverage" bash -c "
  ( cd '$WORK/spec' && '$COVMAP' show queue_links ) | grep -q 'coverage\|segments'"
assert "show queue_links reports the resolved fan-in hub" bash -c "
  out=\$(cd '$WORK/spec' && '$COVMAP' show queue_links)
  printf '%s\n' \"\$out\" | grep -q 'degree max-out 1, max-in 3' \
    && printf '%s\n' \"\$out\" | grep -q 'fan-in:' \
    && printf '%s\n' \"\$out\" | grep -q '3 <- unknown queue.rs:1..15'"
assert "show queue_links wraps the long mapping note" bash -c "
  out=\$(cd '$WORK/spec' && '$COVMAP' show queue_links)
  printf '%s\n' \"\$out\" | grep -q '^    The dequeue operation delegates empty-queue handling to the same' \
    && printf '%s\n' \"\$out\" | grep -q '^    implementation segment while preserving the mapping rationale for review'"
assert "ls-links --resolve keeps the long note on one TAB record" bash -c "
  out=\$(cd '$WORK/spec' && '$COVMAP' ls-links --resolve queue_links)
  [[ \$(printf '%s\n' \"\$out\" | wc -l | tr -d ' ') -eq 3 ]] \
    && printf '%s\n' \"\$out\" | grep -qF \"$DEQUEUE_NOTE\""
# `status` surfaces per-connection coverage.
assert "status reports connection coverage" bash -c "
  ( cd '$WORK/spec' && '$COVMAP' status ) | grep -q 'coverage'"
assert "status remains terse and omits degree detail" bash -c "
  ! ( cd '$WORK/spec' && '$COVMAP' status ) | grep -q 'degree'"
# region show resolves the mapping (the enqueue op maps to the impl).
assert "region show resolves a link to its target loc" bash -c "
  ( cd '$WORK/spec' && '$COVMAP' show main:queue.md ) | grep -q '\->'"

#############################################################################
hr "11. iterator: freeze the covering into a worklist and sweep it  (REQ21)"
#############################################################################
# An iterator freezes the covering into an ordered, stateful worklist: each
# segment becomes a task you pull one at a time and mark done/skipped. It is
# a snapshot layered OVER the covering — it never mutates segments — so we
# run it here on the clean, refined covering before the drift steps below.
( cd "$WORK/spec" && run iter review new main --order file )
( cd "$WORK/spec" && run iter review status )

# `peek` lists every pending task in order (without advancing) — the set you
# can `choose` among. It is how you survey the worklist before diving in.
( cd "$WORK/spec" && run iter review peek )

assert "peek lists all 7 pending tasks (the choosable set)" \
  bash -c "[[ \$( ( cd '$WORK/spec' && '$COVMAP' iter review peek | wc -l ) ) -eq 7 ]]"

# `next --content` advances AND dumps the frozen bytes in one call — the
# review affordance (documented on next/current/peek alike). Regression
# guard: this must be accepted, not rejected as an unexpected argument.
assert "next --content advances and emits frozen content" \
  bash -c "
    out=\$( cd '$WORK/spec' && '$COVMAP' iter review next --content )
    # more than one line = handle line + at least one content line
    [[ \$(printf '%s\n' \"\$out\" | wc -l) -gt 1 ]]
  "
# That advanced onto (and printed) the first task; resolve it so the
# canonical loop below picks up cleanly from the next pending task.
( cd "$WORK/spec" && "$COVMAP" iter review done -m "audited (with content)" >/dev/null )

# The manual controls — shown on a throwaway `plan` iterator so the `review`
# sweep below stays pristine (iterators are cheap, independent overlays).
# `choose` jumps the cursor to any task by @name, out of file order; `skip`
# resolves it as a deliberate "not this one" with a reason; `rewind` undoes a
# resolution, putting the task back to pending. A jump + skip can never
# strand a task — rewind (or a later `next`) always brings it back.
( cd "$WORK/spec" && run iter plan new main )
( cd "$WORK/spec" && run iter plan choose @enqueue-op )
( cd "$WORK/spec" && run iter plan skip -m "trivial — covered elsewhere" )
( cd "$WORK/spec" && run iter plan status )       # 1/7 resolved (0 done, 1 skipped) ...
( cd "$WORK/spec" && run iter plan rewind @enqueue-op )
( cd "$WORK/spec" && run iter plan status )       # 0/7 resolved (0 done, 0 skipped) ...

assert "rewind undid the skip (0 skipped on the plan iterator)" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' iter plan status | grep -q '0 skipped' )"

( cd "$WORK/spec" && run iter plan rm )            # discard the scratch iterator

# The canonical loop: `next` hands out the first pending task and is refused
# until it is resolved, so every segment is visited exactly once. We `done`
# each task with a one-line note (skipping nothing here). The loop body is
# kept comment-free: an interactive zsh treats a pasted `#` as a command,
# not a comment, which would leave the current task unresolved and spin.
( cd "$WORK/spec" && bash -c '
  until "'"$COVMAP"'" iter review finished; do
    line=$("'"$COVMAP"'" iter review next)
    handle=$(printf "%s" "$line" | cut -f1)
    "'"$COVMAP"'" iter review done -m "audited $handle" >/dev/null
  done
' )
( cd "$WORK/spec" && run iter review status )
# `current` is the resume affordance: it reprints the cursor's task in any
# state, so a crashed/compacted agent recovers exactly where it stood.
( cd "$WORK/spec" && run iter review current )

assert "iterator reports finished (every task resolved)" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' iter review finished )"
assert "the sweep did not mutate the covering (fsck still clean)" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' fsck )"

# An iterator can be filtered to one label — e.g. a backfill of only the
# not-yet-refined `unknown` segments. `alt` (step 9) is the whole-file
# covering: two `unknown` segments (queue.md, errors.md), so this freezes
# exactly those two tasks, narrowing the worklist to the backlog.
( cd "$WORK/spec" && run iter backfill new alt --label unknown )
( cd "$WORK/spec" && run iter )        # bare `iter` lists every iterator

assert "label-filtered iterator froze only the unknown segments" \
  bash -c "[[ \$( ( cd '$WORK/spec' && '$COVMAP' iter backfill status | grep -oE '/[0-9]+' | head -1 | tr -d / ) ) -eq 2 ]]"

# Delete the iterators; the covering is untouched (they are pure overlays).
( cd "$WORK/spec" && run iter review rm )
( cd "$WORK/spec" && run iter backfill rm )

#############################################################################
hr "12. T5-style drift: in-place edit + recut, link survives  (REQ13, REQ14)"
#############################################################################
# Tighten the Dequeue wording ("...or None when empty." -> "...or None
# when the queue is empty."). Same number of lines -> segment drift
# with intact boundaries. The @dequeue-op link does NOT break.
cp "$FILES/queue.reworded.md" "$WORK/spec/queue.md"
echo "  edit (files/queue.md -> files/queue.reworded.md):"
diff "$FILES/queue.md" "$FILES/queue.reworded.md" | sed 's/^/    /' || true
( cd "$WORK/spec" && run status main )
echo "  (note: queue_links still shows 0 broken — the link is by @name)"
DEQ=$( cd "$WORK/spec" && "$COVMAP" ls -l main | awk -F'\t' '$5=="@dequeue-op"{print $1}' )
( cd "$WORK/spec" && run recut "main:$DEQ" )
( cd "$WORK/spec" && run status main )

assert "after recut: drift clean" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' status main | grep -q 'drift: clean' )"
assert "queue_links still 3 links, 0 broken" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' status main | grep -qE 'queue_links: .*: 3 links, 0 broken' )"

#############################################################################
hr "13. T6-style drift: insert lines, diff-remap re-fits boundaries  (REQ13)"
#############################################################################
# Add two lines to the preamble; everything below shifts down. recut
# --remap re-fits every line-aligned segment automatically.
cp "$FILES/queue.inserted.md" "$WORK/spec/queue.md"
echo "  edit (files/queue.reworded.md -> files/queue.inserted.md):"
diff "$FILES/queue.reworded.md" "$FILES/queue.inserted.md" | sed 's/^/    /' || true
( cd "$WORK/spec" && run status main )
# No address → remap the default (HEAD = main) covering, every file.
( cd "$WORK/spec" && run recut --remap )
( cd "$WORK/spec" && run status main )

assert "after remap: drift clean" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' status main | grep -q 'drift: clean' )"
assert "after remap: still 3 links, 0 broken" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' status main | grep -qE 'queue_links: .*: 3 links, 0 broken' )"

#############################################################################
hr "14. T7-style drift: move a section, fingerprint proposes its new home"
#############################################################################
# Move the Enqueue section (the @enqueue-op segment) to the END of the file.
# Now the segment that was 2nd in the covering would have to map *after*
# Dequeue and Size — which breaks the monotonic line map, so diff-remap
# cannot place it and instead prints a propose-only relocation guess.
# Nothing is mutated by the remap.
cp "$FILES/queue.moved.md" "$WORK/spec/queue.md"
echo "  queue.md after moving the Enqueue section to the end:"
nl -ba "$WORK/spec/queue.md" | sed 's/^/    /'
( cd "$WORK/spec" && run status main )
echo "  (recut --remap leaves the file untouched and PROPOSES a location:)"
( cd "$WORK/spec" && "$COVMAP" recut --remap || true )

assert "the moved @enqueue-op segment is proposed, not silently re-fit" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' recut --remap 2>&1 | grep -q '@enqueue-op' )"
assert "remap left the covering unmutated for the moved file (still drifted)" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' status main | grep -q 'file(s) edited' )"

#############################################################################
hr "15. prune: drop a genuinely broken link  (REQ14)"
#############################################################################
# Demonstrate prune on a deliberately broken link: clear a segment's name,
# which deletes that segment identity, breaking its link. Then prune drops it.
( cd "$WORK/spec" && run name "main:$( cd "$WORK/spec" && "$COVMAP" ls -l main | awk -F'\t' '$5=="@queuefull-clause"{print $1}' )" - )
( cd "$WORK/spec" && run fsck --broken-links || true )
( cd "$WORK/spec" && run prune queue_links )
( cd "$WORK/spec" && run status main )

#############################################################################
hr "16. render the HTML report  (REQ20)"
#############################################################################
( cd "$WORK/spec" && run html )
echo "  open: $WORK/spec/.covmap/report.html"

#############################################################################
hr "Done"
#############################################################################
cat <<EOF
All tutorial steps ran successfully.

Workspace kept at: $WORK
  - spec covering:  $WORK/spec/.covmap/
  - impl covering:  $WORK/impl_rs/.covmap/
  - HTML report:    $WORK/spec/.covmap/report.html

Re-run any time (it resets _work/ first):  ./run_tutorial.sh
Remove the workspace:                       ./run_tutorial.sh clean
EOF

if [[ "${KEEP:-0}" != "1" && "${1:-}" != "keep" ]]; then
  : # _work/ is intentionally kept by default so you can open the report.
fi
