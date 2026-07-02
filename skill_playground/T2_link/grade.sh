#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${COVMAP_BIN:-}" || ! -x "$COVMAP_BIN" ]]; then
  COVMAP_BIN="$(git -C "$(pwd)" rev-parse --show-toplevel 2>/dev/null)/covmap/target/release/covmap"
fi

pass=0; fail=0
check() {
  local msg="$1"; shift
  if "$@" >/dev/null 2>&1; then
    echo "  PASS  $msg"; pass=$((pass+1))
  else
    echo "  FAIL  $msg"; fail=$((fail+1))
  fi
}

echo "== T2 link =="
check "queue_links exists"           test -f spec/.covmap/connections/queue_links
check "exactly 3 links"              bash -c "[[ \$( ( cd spec && \"$COVMAP_BIN\" ls-links queue_links | wc -l ) ) -eq 3 ]]"
# Single impl hash
IMPL_H=$(cd impl_rs && "$COVMAP_BIN" ls-segments)
check "all links reference impl hash" bash -c "
  count=\$( ( cd spec && \"$COVMAP_BIN\" ls-links queue_links | awk '{print \$2}' | grep -c '^${IMPL_H}\$' ) )
  [[ \$count -eq 3 ]]
"
check "fsck passes (spec)"           bash -c "cd spec && \"$COVMAP_BIN\" fsck"
check "fsck passes (impl)"           bash -c "cd impl_rs && \"$COVMAP_BIN\" fsck"

echo
echo "$pass passed, $fail failed"
exit $(( fail > 0 ? 1 : 0 ))
