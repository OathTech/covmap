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
  [[ \$(grep -cvE '^#' '$REF') -eq 5 ]]
"

# Complete enumeration: nothing left pending.
check "every task is resolved (finished)" bash -c "
  cd spec && \"$COVMAP_BIN\" iter '$ITER' finished
"
check "no task is still pending" bash -c "
  ! grep -qE $'\tpending' '$REF'
"

# Each of the four named endpoint tasks carries a non-empty verdict note.
# Ref columns: hash file a b label name state [note] — so a task with a
# name (col 6) that is resolved must have an 8th (note) field.
for ep in get-widgets post-widgets delete-widget healthz; do
  check "@$ep carries a verdict note" bash -c "
    awk -F'\t' -v n='$ep' '\$6==n && NF>=8 && \$8!=\"\" {ok=1} END{exit ok?0:1}' '$REF'
  "
done

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
