#!/usr/bin/env bash
# Pre-cover both repos and split spec/buffer.md at its two `##`
# headings, so the agent can focus on the character-level split + link.
set -euo pipefail

( cd spec    && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null

# Refine spec/buffer.md at the heading boundaries (lines 5 and 9), so
# the Capacity section (lines 5-8) is its own line-level segment.
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'buffer.md' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H" 5 "preamble" "Capacity" ) >/dev/null
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'buffer.md:5' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H" 9 "Capacity" "Push" ) >/dev/null

( cd spec && "$COVMAP_BIN" connect main "$(pwd)/../impl_rs:main" buffer_links ) >/dev/null

echo "Setup done. spec/buffer.md is split into preamble / Capacity / Push;"
echo "buffer_links is an empty connection from spec/main to impl_rs/main."
