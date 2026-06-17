#!/usr/bin/env bash
# Pre-bootstrap both repos and split spec/queue.md at its three `##`
# headings, so the agent can focus on linking.
set -euo pipefail

( cd spec    && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null

# Refine spec/queue.md: split at lines 5, 9, 14 (the three `##` headings).
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'queue.md' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H"  5 "preamble"  "Enqueue" ) >/dev/null
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'queue.md:5' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H"  9 "Enqueue"   "Dequeue" ) >/dev/null
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'queue.md:9' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H" 14 "Dequeue"   "Length" ) >/dev/null

( cd spec && "$COVMAP_BIN" connect main "$(pwd)/../impl_rs:main" queue_links ) >/dev/null
