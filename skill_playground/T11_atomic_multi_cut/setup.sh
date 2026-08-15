#!/usr/bin/env bash
set -euo pipefail

( cd spec && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null

echo "Setup done. spec/service.md is one whole-file unknown segment."
echo "Refine it with one atomic multi-cut, then apply labels separately."
