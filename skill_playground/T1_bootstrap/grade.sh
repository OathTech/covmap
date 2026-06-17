#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${COVMAP_BIN:-}" || ! -x "$COVMAP_BIN" ]]; then
  COVMAP_BIN="$(git -C "$(pwd)" rev-parse --show-toplevel 2>/dev/null)/covmap/target/release/covmap"
fi
if [[ ! -x "$COVMAP_BIN" ]]; then
  echo "Cannot find covmap binary (set COVMAP_BIN)." >&2
  exit 2
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

echo "== T1 bootstrap =="
check "spec/.covmap/ exists"          test -d spec/.covmap
check "impl_rs/.covmap/ exists"       test -d impl_rs/.covmap
check "spec HEAD names a covering"    test -s spec/.covmap/HEAD
check "impl_rs HEAD names a covering" test -s impl_rs/.covmap/HEAD
check "spec covering 'main' exists"   test -f spec/.covmap/coverings/main
check "impl covering 'main' exists"   test -f impl_rs/.covmap/coverings/main
check "spec covering >= 6 segments"   bash -c "[[ \$( ( cd spec && \"$COVMAP_BIN\" ls-segments | wc -l ) ) -ge 6 ]]"
check "impl covering == 1 segment"    bash -c "[[ \$( ( cd impl_rs && \"$COVMAP_BIN\" ls-segments | wc -l ) ) -eq 1 ]]"

echo
echo "$pass passed, $fail failed"
exit $(( fail > 0 ? 1 : 0 ))
