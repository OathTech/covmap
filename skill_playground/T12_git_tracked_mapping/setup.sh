#!/usr/bin/env bash
set -euo pipefail

mkdir -p author/spec author/impl review/deps
cp spec.md author/spec/spec.md
cp impl.rs author/impl/impl.rs

(
  cd author/spec
  "$COVMAP_BIN" init >/dev/null
  printf '/unused-default/\n' > .covmap/include
  mkdir -p .covmap/selections/main
  printf '/spec.md\n' > .covmap/selections/main/include
  "$COVMAP_BIN" cover >/dev/null
  hash=$("$COVMAP_BIN" ls-segments main)
  "$COVMAP_BIN" name "main:$hash" request-contract >/dev/null
)
(
  cd author/impl
  "$COVMAP_BIN" init >/dev/null
  printf '/impl.rs\n' > .covmap/include
  "$COVMAP_BIN" cover >/dev/null
  hash=$("$COVMAP_BIN" ls-segments main)
  "$COVMAP_BIN" name "main:$hash" request-handler >/dev/null
)
(
  cd author/spec
  "$COVMAP_BIN" peer bind impl ../impl >/dev/null
  "$COVMAP_BIN" connect main impl:main spec_to_impl >/dev/null
  "$COVMAP_BIN" link spec_to_impl @request-contract @request-handler \
    "request contract is implemented by handle" >/dev/null
  git init -q
  git add spec.md .covmap
  git -c user.name=covmap -c user.email=covmap@example.invalid \
    commit -qm "publish specification mapping"
)
(
  cd author/impl
  git init -q
  git add impl.rs .covmap
  git -c user.name=covmap -c user.email=covmap@example.invalid \
    commit -qm "publish implementation covering"
)

git clone -q author/spec review/spec-clone
git clone -q author/impl review/deps/implementation

echo "Setup done. Work from review/spec-clone; its cache and peer binding are absent."
