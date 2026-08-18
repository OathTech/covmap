#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${COVMAP_BIN:-}" || ! -x "$COVMAP_BIN" ]]; then
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

echo "== T12 Git-tracked mapping =="

check "connection bytes are unchanged across checkout locations" \
  cmp author/spec/.covmap/connections/spec_to_impl \
      review/spec-clone/.covmap/connections/spec_to_impl

check "connection stores a peer alias and no absolute target path" bash -c "
  grep -q '^# target_peer=impl$' review/spec-clone/.covmap/connections/spec_to_impl &&
  ! grep -q '^# target_repo=' review/spec-clone/.covmap/connections/spec_to_impl
"

check "the clone's local covering object was hydrated" bash -c "
  [[ \$(find review/spec-clone/.covmap/objects -type f | wc -l | tr -d ' ') -eq 1 ]]
"

check "the review checkout has the local impl binding" bash -c "
  cd review/spec-clone &&
  '$COVMAP_BIN' peer ls | grep -q \$'^impl\\tbound\\t'
"

check "full integrity verification passes" \
  bash -c "cd review/spec-clone && '$COVMAP_BIN' fsck"

check "mapping resolves in inspection" bash -c "
  cd review/spec-clone &&
  '$COVMAP_BIN' show spec_to_impl | grep -q 'request contract is implemented by handle'
"

check "local cache and binding remain ignored and Git-clean" bash -c "
  cd review/spec-clone &&
  [[ -z \$(git status --short) ]] &&
  ! git ls-files .covmap/objects .covmap/local | grep -q .
"

echo
echo "$pass passed, $fail failed"
exit $((fail > 0 ? 1 : 0))
