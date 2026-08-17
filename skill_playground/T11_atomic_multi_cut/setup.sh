#!/usr/bin/env bash
set -euo pipefail

( cd spec && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null
rm -rf spec/.covmap/objects/*

echo "Setup done. spec/service.md is one whole-file unknown segment."
echo "The live object cache is empty; verify and hydrate it before refinement."
