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

echo "== T9 bounded drill-down + label lens =="

# The map instantiated: 4 links, drift-free, endpoints resolve.
check "fsck passes (spec)" bash -c "cd spec && \"$COVMAP_BIN\" fsck"
check "apimap has 4 links" bash -c "
  [[ \$( ( cd spec && \"$COVMAP_BIN\" ls-links apimap | wc -l ) ) -eq 4 ]]
"

# (1) Single-segment drill-down resolves the DELETE mapping: show of the
# spec-delete segment names its handler and the handler's location.
check "show @spec-delete resolves to the delete handler" bash -c "
  cd spec && \"$COVMAP_BIN\" show main:@spec-delete | grep -q 'impl-delete\|handlers.rs:19'
"
check "show @spec-delete lists exactly one outgoing link" bash -c "
  cd spec && \"$COVMAP_BIN\" show main:@spec-delete | grep -q '1 outgoing'
"

# (2) The label lens returns BOTH auth endpoints, each with its mapping.
check "label lens finds the two auth endpoints" bash -c "
  cd spec
  out=\$(\"$COVMAP_BIN\" show --label auth main)
  echo \"\$out\" | grep -q 'spec-post' && echo \"\$out\" | grep -q 'spec-delete'
"
check "label lens resolves auth endpoints to their handlers" bash -c "
  cd spec
  out=\$(\"$COVMAP_BIN\" show --label auth main)
  echo \"\$out\" | grep -q 'impl-create\|handlers.rs:8' \
    && echo \"\$out\" | grep -q 'impl-delete\|handlers.rs:19'
"

# (3) status surfaces coverage for the connection (source-total: all 4
# endpoints linked, plus the preamble unlinked = 4/5).
check "status reports apimap coverage" bash -c "
  cd spec && \"$COVMAP_BIN\" status | grep -q 'coverage'
"

# (4) region view resolves the mapping for a file slice.
check "region show resolves a link" bash -c "
  cd spec && \"$COVMAP_BIN\" show main:api.md:18..24 | grep -q '\\->'
"

# The label lens is the intended tool: an agent must not have needed to
# mutate anything. Covering is intact (5 segments, four names present).
check "spec covering still has 5 segments" bash -c "
  [[ \$( ( cd spec && \"$COVMAP_BIN\" ls main | wc -l ) ) -eq 5 ]]
"

echo
echo "$pass passed, $fail failed"
exit $(( fail > 0 ? 1 : 0 ))
