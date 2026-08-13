#!/usr/bin/env bash
set -euo pipefail

"$COVMAP_BIN" init >/dev/null
"$COVMAP_BIN" cover >/dev/null
"$COVMAP_BIN" split main:guide.md:1 3 preamble source >/dev/null
"$COVMAP_BIN" split main:guide.md:3 7 source target >/dev/null
"$COVMAP_BIN" name main:guide.md:3 old-source >/dev/null
"$COVMAP_BIN" name main:guide.md:7 target >/dev/null
"$COVMAP_BIN" connect main main audit >/dev/null
"$COVMAP_BIN" link audit @old-source @target "same-repo rationale" >/dev/null
