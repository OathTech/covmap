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

echo "== T3 drift repair =="
check "cache_links exists"           test -f spec/.covmap/connections/cache_links
check "exactly 3 links"              bash -c "[[ \$( ( cd spec && \"$COVMAP_BIN\" ls-links cache_links | wc -l ) ) -eq 3 ]]"
check "no broken links from fsck"    bash -c "[[ -z \"\$( ( cd spec && \"$COVMAP_BIN\" fsck --broken-links ) )\" ]]"
check "status shows 0 broken"        bash -c "( cd spec && \"$COVMAP_BIN\" status ) | grep -qE 'cache_links: .*: 3 links, 0 broken'"

echo
echo "$pass passed, $fail failed"
exit $(( fail > 0 ? 1 : 0 ))
