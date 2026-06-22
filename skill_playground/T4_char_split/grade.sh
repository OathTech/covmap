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

CLAUSE="It must be a power of two so that the implementation can mask indices instead of taking a modulus."

echo "== T4 character-level split =="

# The covering must still be a valid partition after the edits.
check "fsck passes (spec)"   bash -c "cd spec && \"$COVMAP_BIN\" fsck"
check "fsck passes (impl)"   bash -c "cd impl_rs && \"$COVMAP_BIN\" fsck"

# There must be a segment whose range carries a character column on
# buffer.md line 7 — i.e. a genuine character-level boundary, not a
# whole-line split.
check "a buffer.md:7 segment has a character column" bash -c "
  cd spec && \"$COVMAP_BIN\" ls -l \
    | awk -F'\t' '{print \$3}' \
    | grep -Eq 'buffer\.md:(7:[0-9]+\.\.|[0-9:]+\.\.7:[0-9]+)'
"

# Exactly one segment's content is the normative clause (ignoring any
# trailing whitespace / newline). This is the clause, isolated.
check "exactly one segment isolates the clause" bash -c "
  cd spec
  n=0
  while read -r h; do
    c=\$(\"$COVMAP_BIN\" cat-segment \"\$h\" | sed -e 's/[[:space:]]*\$//')
    [[ \"\$c\" == \"$CLAUSE\" ]] && n=\$((n+1))
  done < <(\"$COVMAP_BIN\" ls-segments main)
  [[ \$n -eq 1 ]]
"

# That clause segment must be linked to the implementation.
check "buffer_links exists"  test -f spec/.covmap/connections/buffer_links
check "clause segment is linked to impl" bash -c "
  cd spec
  IMPL_H=\$(cd ../impl_rs && \"$COVMAP_BIN\" ls-segments)
  # find the clause segment hash
  clause_h=''
  while read -r h; do
    c=\$(\"$COVMAP_BIN\" cat-segment \"\$h\" | sed -e 's/[[:space:]]*\$//')
    [[ \"\$c\" == \"$CLAUSE\" ]] && clause_h=\$h
  done < <(\"$COVMAP_BIN\" ls-segments main)
  [[ -n \"\$clause_h\" ]] || exit 1
  \"$COVMAP_BIN\" ls-pairs buffer_links | awk -F'\t' '{print \$1\"\t\"\$2}' \
    | grep -qF \"\$clause_h\"\$'\t'\"\$IMPL_H\"
"

echo
echo "$pass passed, $fail failed"
exit $(( fail > 0 ? 1 : 0 ))
