#!/usr/bin/env bash
# Pre-cover both repos and split spec/queue.md at its three `##`
# headings, so the agent can focus on linking.
set -euo pipefail

( cd spec    && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null

# Refine spec/queue.md atomically at its three `##` headings, then compose
# labels explicitly.
( cd spec && "$COVMAP_BIN" split main:queue.md 5 9 14 ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:queue.md:1  preamble ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:queue.md:5  Enqueue  ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:queue.md:9  Dequeue  ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:queue.md:14 Length   ) >/dev/null

( cd spec && "$COVMAP_BIN" peer bind impl ../impl_rs ) >/dev/null
( cd spec && "$COVMAP_BIN" connect main impl:main queue_links ) >/dev/null
