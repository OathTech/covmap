#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${COVMAP_BIN:-}" || ! -x "$COVMAP_BIN" ]]; then
  COVMAP_BIN="$(git -C "$(pwd)" rev-parse --show-toplevel 2>/dev/null)/covmap/target/release/covmap"
fi

pass=0
fail=0
check() {
  local msg="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    echo "  PASS  $msg"
    pass=$((pass + 1))
  else
    echo "  FAIL  $msg"
    fail=$((fail + 1))
  fi
}

echo "== T10 connection lifecycle =="
check "audit connection remains" test -f .covmap/connections/audit
check "scratch connection was deleted" test ! -e .covmap/connections/scratch
check "source segment has its new name" \
  bash -c "'$COVMAP_BIN' ls -l main | awk -F '\t' '\$5==\"@current-source\"{found=1} END{exit !found}'"
check "old source name is absent" \
  bash -c "! '$COVMAP_BIN' ls -l main | grep -qF '@old-source'"
check "link and note were preserved" \
  bash -c "[[ \"\$('$COVMAP_BIN' ls-links audit)\" == \$'@current-source\\t@target\\tsame-repo rationale' ]]"
check "status identifies audit as a self-map" \
  bash -c "'$COVMAP_BIN' status | grep -qF 'audit: main -> main (self-map)'"
check "fsck passes" "$COVMAP_BIN" fsck

echo
echo "$pass passed, $fail failed"
exit $((fail > 0 ? 1 : 0))
