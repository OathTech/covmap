#!/usr/bin/env bash
# Cover, refine spec/api.md at its headings, NAME + link the Read and
# Delete sections, then INSERT lines into the Create section.
#
# The insertion is a boundary-moving edit: it pushes the Read and Delete
# sections down by several lines, so their recorded ranges no longer fit
# (plain `recut` would refuse). The fix is `recut --remap`, which diffs
# the reconstructed old file against the new one and re-fits every
# downstream segment's boundaries automatically, preserving names so the
# @read / @delete links carry forward untouched.
set -euo pipefail

( cd spec    && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null

# Refine spec/api.md atomically at the three `##` headings, then label the
# resulting sections explicitly.
( cd spec && "$COVMAP_BIN" split main:api.md 5 9 13 ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:api.md:1  preamble ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:api.md:5  Create   ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:api.md:9  Read     ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:api.md:13 Delete   ) >/dev/null

# Name the Read and Delete segments and link them to the implementation.
( cd spec && "$COVMAP_BIN" name "main:$( "$COVMAP_BIN" grep -l -c main '## Read'   | head -1 )" read-ep   ) >/dev/null
( cd spec && "$COVMAP_BIN" name "main:$( "$COVMAP_BIN" grep -l -c main '## Delete' | head -1 )" delete-ep ) >/dev/null
IMPL_H=$( cd impl_rs && "$COVMAP_BIN" ls-segments )
( cd spec && "$COVMAP_BIN" connect main "$(pwd)/../impl_rs:main" api_links ) >/dev/null
( cd spec && "$COVMAP_BIN" link api_links @read-ep   "$IMPL_H" "fn read"   ) >/dev/null
( cd spec && "$COVMAP_BIN" link api_links @delete-ep "$IMPL_H" "fn delete" ) >/dev/null

# Boundary-moving drift: expand the Create section with extra prose.
# This pushes Read and Delete down; their recorded ranges no longer fit.
python3 - <<'PY'
from pathlib import Path
p = Path("spec/api.md")
t = p.read_text()
t = t.replace(
    "`POST /items` creates an item from the JSON body and returns its id.",
    "`POST /items` creates an item from the JSON body and returns its id.\n"
    "The body must be a JSON object.\n"
    "Duplicate ids are rejected with 409.",
)
p.write_text(t)
PY

echo "Setup done. spec/api.md gained 2 lines in the Create section, so the"
echo "Read and Delete sections moved down. Use 'recut --remap' to re-fit."
