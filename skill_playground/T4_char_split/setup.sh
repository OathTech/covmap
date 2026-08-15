#!/usr/bin/env bash
# Pre-cover both repos and split spec/buffer.md at its two `##`
# headings, so the agent can focus on the character-level split + link.
set -euo pipefail

( cd spec    && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null

# Refine spec/buffer.md atomically at the heading boundaries (lines 5 and 9),
# then label the resulting sections explicitly.
( cd spec && "$COVMAP_BIN" split main:buffer.md 5 9 ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:buffer.md:1 preamble ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:buffer.md:5 Capacity ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:buffer.md:9 Push     ) >/dev/null

( cd spec && "$COVMAP_BIN" connect main "$(pwd)/../impl_rs:main" buffer_links ) >/dev/null

echo "Setup done. spec/buffer.md is split into preamble / Capacity / Push;"
echo "buffer_links is an empty connection from spec/main to impl_rs/main."
