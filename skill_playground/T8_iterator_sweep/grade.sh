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

echo "== T8 iterator sweep (complete enumeration) =="

# Discover the iterator the agent created (any name).
ITER=$(ls spec/.covmap/iterators 2>/dev/null | grep -v '^\.' | grep -v '\.tmp$' | head -1)
if [[ -z "$ITER" ]]; then
  echo "  FAIL  an iterator over the covering exists"
  echo
  echo "0 passed, 1 failed"
  exit 1
fi
echo "  (grading iterator '$ITER')"
REF="spec/.covmap/iterators/$ITER"

# One task per segment: the covering has 5 segments, so must the worklist.
check "iterator has one task per segment (5)" bash -c "
  [[ \$(cd spec && \"$COVMAP_BIN\" iter '$ITER' peek --all | wc -l) -eq 5 ]]
"

# Complete enumeration: nothing left pending.
check "every task is resolved (finished)" bash -c "
  cd spec && \"$COVMAP_BIN\" iter '$ITER' finished
"
check "no task is still pending" bash -c "
  (cd spec && \"$COVMAP_BIN\" iter '$ITER' peek --all) \
    | awk -F'\t' '\$4 == \"pending\" {found=1} END {exit found ? 1 : 0}'
"
check "current defaults to frozen content" bash -c "
  (cd spec && \"$COVMAP_BIN\" iter '$ITER' current) | grep -q '^--- content '
"
check "current --no-content is a single compact row" bash -c "
  [[ \$(cd spec && \"$COVMAP_BIN\" iter '$ITER' current --no-content | wc -l) -eq 1 ]]
"

# Each of the four named endpoint tasks carries a non-empty verdict note.
# `peek --all` columns: handle, label, location, state, optional escaped note.
for ep in get-widgets post-widgets delete-widget healthz; do
  check "@$ep carries a verdict note" bash -c "
    (cd spec && \"$COVMAP_BIN\" iter '$ITER' peek --all) \
      | awk -F'\t' -v h='@$ep' '\$1==h && NF>=5 && \$5!=\"\" {ok=1} END{exit ok?0:1}'
  "
done
check "at least one endpoint verdict has two paragraphs" bash -c "
  (cd spec && \"$COVMAP_BIN\" iter '$ITER' peek --all) \
    | awk -F'\t' '\$1 ~ /^@(get-widgets|post-widgets|delete-widget|healthz)$/ && \$5 ~ /\\\\n\\\\n/ {ok=1} END{exit ok?0:1}'
"

# The iterator is a snapshot layered over the covering: sweeping it must
# not mutate the covering. Structure is intact — still 5 segments and the
# four names present — and fsck is clean.
check "covering still has 5 segments" bash -c "
  [[ \$( ( cd spec && \"$COVMAP_BIN\" ls main | wc -l ) ) -eq 5 ]]
"
check "the four endpoint names are still in the covering" bash -c "
  cd spec
  for n in @get-widgets @post-widgets @delete-widget @healthz; do
    \"$COVMAP_BIN\" ls -l main | grep -qF \"\$n\" || exit 1
  done
"
check "fsck passes (spec)" bash -c "cd spec && \"$COVMAP_BIN\" fsck"

echo
echo "$pass passed, $fail failed"
exit $(( fail > 0 ? 1 : 0 ))
