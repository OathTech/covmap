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

echo "== T5 name-stable links =="

# The Login slot must still carry its name-tag.
check "Login slot is named @login-contract" bash -c "
  cd spec && \"$COVMAP_BIN\" ls -l main | grep -qF '@login-contract'
"

# The connection has exactly one link, and it references the slot by
# NAME (@login-contract) -- not by a bare hash.
check "auth_links has exactly 1 link"  bash -c "
  [[ \$( ( cd spec && \"$COVMAP_BIN\" ls-links auth_links | wc -l ) ) -eq 1 ]]
"
check "the link references @login-contract" bash -c "
  cd spec && \"$COVMAP_BIN\" ls-links auth_links | awk -F'\t' '{print \$1}' | grep -qx '@login-contract'
"

# After the in-place edit + recut, the covering must be clean and the
# link must NOT be broken -- the whole point of name-stable links.
check "drift is clean (slot was recut)" bash -c "
  cd spec && \"$COVMAP_BIN\" status | grep -q 'drift: clean'
"
check "auth_links has 0 broken links"   bash -c "
  cd spec && \"$COVMAP_BIN\" status | grep -qE 'auth_links .*: 1 links, 0 broken'
"
check "no broken links from fsck"       bash -c "
  [[ -z \"\$( ( cd spec && \"$COVMAP_BIN\" fsck --broken-links ) )\" ]]
"
check "fsck passes (spec)"              bash -c "cd spec && \"$COVMAP_BIN\" fsck"

echo
echo "$pass passed, $fail failed"
exit $(( fail > 0 ? 1 : 0 ))
