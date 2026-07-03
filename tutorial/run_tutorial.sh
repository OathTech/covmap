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
# two lines, so the sections are not uniform). We address each segment
# *positionally* — `main:queue.md:<line>` is "the segment of queue.md
# that covers <line>" (unique by Segment Lookup), a stable handle that
# never depends on a segment hash. Each split's SECOND argument is the
# cut line.
( cd "$WORK/spec" && run split main:queue.md:1  5 preamble Enqueue )
( cd "$WORK/spec" && run split main:queue.md:5  10 Enqueue Dequeue )
( cd "$WORK/spec" && run split main:queue.md:10 14 Dequeue Size )
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
( cd "$WORK/spec" && run split main:queue.md:5 10 Enqueue Dequeue )

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
# the chunk you're working on. (On a split the name stays with the top
# half; step 7 clears this temporary name.)
( cd "$WORK/spec" && run split main:errors.md:3 3 head qf-line )
( cd "$WORK/spec" && run name  main:errors.md:3 qf-line )
( cd "$WORK/spec" && run split main:@qf-line "3:$ECOL" QueueFull-code QueueFull-meaning )
( cd "$WORK/spec" && run ls -l )

assert "a character-level segment exists on errors.md line 3" \
  bash -c "( cd '$WORK/spec' && '$COVMAP' ls -l main | grep -qE 'errors\.md:3:[0-9]+' )"

#############################################################################
hr "6. label a segment  (REQ4)"
#############################################################################
# Size is the segment covering queue.md line 14.
( cd "$WORK/spec" && run label main:queue.md:14 reviewed )

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
hr "8. connect spec -> impl and link by name  (REQ2, REQ10, REQ16)"
#############################################################################
( cd "$WORK/spec" && run connect main "$WORK/impl_rs:main" queue_links )
IMPL=$( cd "$WORK/impl_rs" && "$COVMAP" ls-segments )
( cd "$WORK/spec" && run link queue_links @enqueue-op        "$IMPL" "fn enqueue" )
( cd "$WORK/spec" && run link queue_links @dequeue-op        "$IMPL" "fn dequeue" )
( cd "$WORK/spec" && run link queue_links @queuefull-meaning "$IMPL" "QueueFull guard" )
( cd "$WORK/spec" && run ls-links queue_links )

assert "connection has 3 links, all by @name" \
  bash -c "[[ \$( ( cd '$WORK/spec' && '$COVMAP' ls-links queue_links | grep -c '^@' ) ) -eq 3 ]]"

#############################################################################
hr "9. inspect: show, grep, diff, fsck, status  (REQ15, REQ19, REQ20)"
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
hr "10. T5-style drift: in-place edit + recut, link survives  (REQ13, REQ14)"
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
  bash -c "( cd '$WORK/spec' && '$COVMAP' status main | grep -qE 'queue_links .*: 3 links, 0 broken' )"

#############################################################################
hr "11. T6-style drift: insert lines, diff-remap re-fits boundaries  (REQ13)"
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
  bash -c "( cd '$WORK/spec' && '$COVMAP' status main | grep -qE 'queue_links .*: 3 links, 0 broken' )"

#############################################################################
hr "12. T7-style drift: move a section, fingerprint proposes its new home"
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
hr "13. repair: drop a genuinely broken link  (REQ14)"
#############################################################################
# Demonstrate repair on a deliberately broken link: clear a segment's name,
# which deletes that segment identity, breaking its link. Then repair drops it.
( cd "$WORK/spec" && run name "main:$( cd "$WORK/spec" && "$COVMAP" ls -l main | awk -F'\t' '$5=="@queuefull-meaning"{print $1}' )" - )
( cd "$WORK/spec" && run fsck --broken-links || true )
( cd "$WORK/spec" && run repair queue_links )
( cd "$WORK/spec" && run status main )

#############################################################################
hr "14. render the HTML report  (REQ20)"
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
