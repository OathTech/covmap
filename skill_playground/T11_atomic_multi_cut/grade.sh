#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${COVMAP_BIN:-}" || ! -x "$COVMAP_BIN" ]]; then
  COVMAP_BIN="$(git -C "$(pwd)" rev-parse --show-toplevel 2>/dev/null)/covmap/target/release/covmap"
fi
if [[ ! -x "$COVMAP_BIN" ]]; then
  echo "Cannot find covmap binary (set COVMAP_BIN)." >&2
  exit 2
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

echo "== T11 atomic multi-cut =="

check "covering has the five exact ranges in file order" bash -c "
  cd spec
  actual=\$(\"$COVMAP_BIN\" ls-segments -l main |
    awk -F '\t' '{print \$2 \":\" \$3 \"..\" \$4}' |
    paste -sd ',' -)
  [[ \"\$actual\" == 'service.md:1..4,service.md:5..8,service.md:9..13,service.md:14..17,service.md:18..20' ]]
"

check "the requested labels are applied separately" bash -c "
  cd spec
  actual=\$(\"$COVMAP_BIN\" ls-segments -l main |
    awk -F '\t' '{print \$5}' |
    paste -sd ',' -)
  [[ \"\$actual\" == 'preamble,Parse,Validate,Execute,Report' ]]
"

check "stored segments reconstruct service.md byte-for-byte" bash -c "
  cd spec
  tmp=\$(mktemp)
  trap 'rm -f \"\$tmp\"' EXIT
  while IFS= read -r hash; do
    \"$COVMAP_BIN\" cat-segment \"\$hash\"
  done < <(\"$COVMAP_BIN\" ls-segments main) >\"\$tmp\"
  cmp -s \"\$tmp\" service.md
"

check "repository format remains v5" bash -c "
  [[ \"\$(tr -d '\\n' < spec/.covmap/version)\" == 5 ]]
"

check "status reports clean drift" bash -c "
  cd spec && \"$COVMAP_BIN\" status | grep -qF 'drift: clean'
"

check "fsck passes" bash -c "cd spec && \"$COVMAP_BIN\" fsck"

echo
echo "$pass passed, $fail failed"
exit $((fail > 0 ? 1 : 0))
