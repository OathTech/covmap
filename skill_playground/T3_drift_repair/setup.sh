#!/usr/bin/env bash
# Cover, refine, link all three headings, then drift the spec.
set -euo pipefail

( cd spec    && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null

# Refine spec/cache.md at lines 5, 10, 15.
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'cache.md' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H"  5 "preamble" "Get" ) >/dev/null
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'cache.md:5' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H" 10 "Get" "Put" ) >/dev/null
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'cache.md:10' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H" 15 "Put" "Size" ) >/dev/null

( cd spec && "$COVMAP_BIN" connect main "$(pwd)/../impl_rs:main" cache_links ) >/dev/null
IMPL_H=$( cd impl_rs && "$COVMAP_BIN" ls-segments )

# Link Get, Put, Size.
GET=$( cd spec && "$COVMAP_BIN" grep -l -c main "## Get"  | head -1 )
PUT=$( cd spec && "$COVMAP_BIN" grep -l -c main "## Put"  | head -1 )
SIZE=$( cd spec && "$COVMAP_BIN" grep -l -c main "## Size" | head -1 )
( cd spec && "$COVMAP_BIN" link cache_links "$GET"  "$IMPL_H" "Get" )  >/dev/null
( cd spec && "$COVMAP_BIN" link cache_links "$PUT"  "$IMPL_H" "Put" )  >/dev/null
( cd spec && "$COVMAP_BIN" link cache_links "$SIZE" "$IMPL_H" "Size" ) >/dev/null

# Drift: tighten the Put contract. Keeps line count the same, so the
# Put segment's boundaries are still correct — only its bytes (hence
# its hash) change. The right fix is `covmap recut`.
python3 - <<'PY'
from pathlib import Path
p = Path("spec/cache.md")
t = p.read_text()
t = t.replace(
    "least-recently-used entry is dropped.",
    "least-recently-used entry is dropped (LRU).",
)
p.write_text(t)
PY

echo "Setup done. Spec has been edited; the Put segment is now stale."
