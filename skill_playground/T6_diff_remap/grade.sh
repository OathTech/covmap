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

echo "== T6 diff-remap =="

# After remap the covering must be a clean partition of the edited file.
check "drift is clean"           bash -c "cd spec && \"$COVMAP_BIN\" status | grep -q 'drift: clean'"
check "fsck passes (spec)"       bash -c "cd spec && \"$COVMAP_BIN\" fsck"

# The named slots survived: still present and still linked, 0 broken.
check "Read slot still @read-ep"     bash -c "cd spec && \"$COVMAP_BIN\" ls -l main | grep -qF '@read-ep'"
check "Delete slot still @delete-ep" bash -c "cd spec && \"$COVMAP_BIN\" ls -l main | grep -qF '@delete-ep'"
check "api_links 2 links, 0 broken"  bash -c "cd spec && \"$COVMAP_BIN\" status | grep -qE 'api_links .*: 2 links, 0 broken'"
check "no broken links from fsck"    bash -c "[[ -z \"\$( ( cd spec && \"$COVMAP_BIN\" fsck --broken-links ) )\" ]]"

# The remap must have re-fit boundaries: the @read-ep slot's content is
# now exactly the Read section (heading + body), proving boundaries
# followed the upstream insertion rather than staying put.
check "Read slot content is the Read section" bash -c "
  cd spec
  h=\$( \"$COVMAP_BIN\" ls -l main | awk -F'\t' '\$5==\"@read-ep\"{print \$1}' )
  c=\$( \"$COVMAP_BIN\" cat-segment \"\$h\" )
  [[ \"\$c\" == *'## Read'* && \"\$c\" == *'GET /items/{id}'* && \"\$c\" != *'## Delete'* ]]
"

# Reconstruction from the covering must reproduce the file byte-for-byte.
check "covering reconstructs the file" bash -c "
  cd spec
  \"$COVMAP_BIN\" ls-segments main | while read h; do \"$COVMAP_BIN\" cat-segment \"\$h\"; done > /tmp/t6_recon.txt
  cmp -s /tmp/t6_recon.txt api.md
"

echo
echo "$pass passed, $fail failed"
exit $(( fail > 0 ? 1 : 0 ))
