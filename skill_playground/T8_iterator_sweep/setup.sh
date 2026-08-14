#!/usr/bin/env bash
# Cover spec/, refine api.md into one segment per section (preamble + the
# four endpoints), and name each endpoint segment. This leaves a covering
# whose segments are a natural per-endpoint worklist: the agent freezes it
# into an iterator and sweeps every endpoint exactly once, recording a
# verdict per task.
#
# Nothing is linked and there is no drift here: T8 is about *enumerating* a
# covering as a durable, resumable work plan, not about repair.
set -euo pipefail

( cd spec && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null

# The `##` headings sit at lines 5, 10, 15, 20. Insert every boundary
# atomically, then apply labels explicitly.
( cd spec && "$COVMAP_BIN" split main:api.md 5 10 15 20 ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:api.md:1  preamble      ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:api.md:5  get-widgets   ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:api.md:10 post-widgets  ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:api.md:15 delete-widget ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:api.md:20 healthz       ) >/dev/null

# Name the four endpoint segments (the preamble stays anonymous). A name is
# a stable, readable handle a task carries into the worklist.
( cd spec && "$COVMAP_BIN" name main:api.md:5  get-widgets   ) >/dev/null
( cd spec && "$COVMAP_BIN" name main:api.md:10 post-widgets  ) >/dev/null
( cd spec && "$COVMAP_BIN" name main:api.md:15 delete-widget ) >/dev/null
( cd spec && "$COVMAP_BIN" name main:api.md:20 healthz       ) >/dev/null

echo "Setup done. spec/main has 5 segments (preamble + 4 named endpoints)."
echo "Freeze it into an iterator and audit every endpoint exactly once."
