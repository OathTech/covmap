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

# The map instantiated: 5 links, drift-free, endpoints resolve.
check "fsck passes (spec)" bash -c "cd spec && \"$COVMAP_BIN\" fsck"
check "apimap has 5 links" bash -c "
  [[ \$( ( cd spec && \"$COVMAP_BIN\" ls-links apimap | wc -l ) ) -eq 5 ]]
"

# (1) Multi-region listing returns both files once in covering order.
check "multi-region ls includes api.md and errors.md without duplicates" bash -c "
  cd spec
  out=\$(\"$COVMAP_BIN\" ls -l main errors.md api.md:18..24 api.md:20..24)
  [[ \$(printf '%s\n' \"\$out\" | wc -l | tr -d ' ') -eq 2 ]] \
    && [[ \$(printf '%s\n' \"\$out\" | grep -c 'api.md') -eq 1 ]] \
    && [[ \$(printf '%s\n' \"\$out\" | grep -c 'errors.md') -eq 1 ]] \
    && ! printf '%s\n' \"\$out\" | grep -q 'api.md:1..4'
"

# (2) Single-segment drill-down resolves the DELETE mapping: show of the
# spec-delete segment names its handler and the handler's location.
check "show @spec-delete resolves to the delete handler" bash -c "
  cd spec && \"$COVMAP_BIN\" show main:@spec-delete | grep -q 'impl-delete\|handlers.rs:19'
"
check "show @spec-delete lists exactly one outgoing link" bash -c "
  cd spec && \"$COVMAP_BIN\" show main:@spec-delete | grep -q '1 outgoing'
"

# (3) The label lens returns BOTH auth endpoints, each with its mapping.
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

# (4) status surfaces coverage for the connection.
check "status reports apimap coverage" bash -c "
  cd spec && \"$COVMAP_BIN\" status | grep -q 'coverage'
"

# (5) region view resolves the mapping for a file slice.
check "region show resolves a link" bash -c "
  cd spec && \"$COVMAP_BIN\" show main:api.md:18..24 | grep -q '\\->'
"

# (6) Connection review reports the resolved hub and wraps the long note.
check "connection review reports the create-handler fan-in hub" bash -c "
  cd spec
  out=\$(\"$COVMAP_BIN\" show apimap)
  printf '%s\n' \"\$out\" | grep -q 'degree max-out 1, max-in 2' \
    && printf '%s\n' \"\$out\" | grep -q 'fan-in:' \
    && printf '%s\n' \"\$out\" | grep -q '2 <- create-fn handlers.rs:8..18'
"
check "connection review wraps the error-contract note" bash -c "
  cd spec
  out=\$(\"$COVMAP_BIN\" show apimap)
  printf '%s\n' \"\$out\" | grep -q '^    Authentication failures from the shared error contract' \
    && printf '%s\n' \"\$out\" | grep -q '^    create handler before request validation continues'
"

# The label lens is the intended tool: an agent must not have needed to
# mutate anything. Covering is intact (6 segments, five names present).
check "spec covering still has 6 segments" bash -c "
  [[ \$( ( cd spec && \"$COVMAP_BIN\" ls main | wc -l ) ) -eq 6 ]]
"

echo
echo "$pass passed, $fail failed"
exit $(( fail > 0 ? 1 : 0 ))
