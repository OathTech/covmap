#!/usr/bin/env bash
set -uo pipefail

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

echo "== T7 fingerprint proposal (propose-only) =="

# Record the covering ref before running remap, to prove propose-only.
before=$(cat spec/.covmap/coverings/main)

# Run diff-remap; capture combined output. The moved slot cannot be
# placed, so this prints a fingerprint proposal and exits non-zero.
out=$( ( cd spec && "$COVMAP_BIN" recut --remap ) 2>&1 || true )

check "reports an unresolved slot"       bash -c "printf '%s' \"\$0\" | grep -q 'unresolved'" "$out"
check "names @audit-retention in proposal" bash -c "printf '%s' \"\$0\" | grep -q '@audit-retention'" "$out"
check "proposal points at the moved segment (lines 12-15)" bash -c "
  printf '%s' \"\$0\" | grep -Eq 'policy\.md:1[0-9]\.\.1[0-9].*match'
" "$out"
check "proposal includes a match percentage" bash -c "printf '%s' \"\$0\" | grep -Eq '[0-9]+% match'" "$out"

# Propose-only: the covering ref must be byte-identical to before.
after=$(cat spec/.covmap/coverings/main)
check "covering was NOT mutated (propose-only)" bash -c "[[ \"\$0\" == \"\$1\" ]]" "$before" "$after"

# The slot is still named and the link still exists (nothing dropped).
check "@audit-retention slot still present" bash -c "
  cd spec && \"$COVMAP_BIN\" ls -l main | grep -qF '@audit-retention'
"
check "policy_links still has its link"     bash -c "
  [[ \$( ( cd spec && \"$COVMAP_BIN\" ls-links policy_links | wc -l ) ) -eq 1 ]]
"

echo
echo "$pass passed, $fail failed"
exit $(( fail > 0 ? 1 : 0 ))
