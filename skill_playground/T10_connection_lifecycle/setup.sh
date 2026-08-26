#!/usr/bin/env bash
set -euo pipefail

"$COVMAP_BIN" init >/dev/null
"$COVMAP_BIN" cover >/dev/null
"$COVMAP_BIN" split main:guide.md 3 7 >/dev/null
"$COVMAP_BIN" label main:guide.md:1 preamble >/dev/null
"$COVMAP_BIN" label main:guide.md:3 source >/dev/null
"$COVMAP_BIN" label main:guide.md:7 target >/dev/null
"$COVMAP_BIN" name main:guide.md:3 old-source >/dev/null
"$COVMAP_BIN" name main:guide.md:7 target >/dev/null
"$COVMAP_BIN" connect main main audit >/dev/null
"$COVMAP_BIN" link audit @old-source @target -m "same-repo rationale" >/dev/null
