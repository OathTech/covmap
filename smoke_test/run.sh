#!/usr/bin/env bash
# Smoke test for covmap. Exercises:
#   init -> cover -> split -> merge -> connect -> link -> fsck
#   -> label -> drift after edit -> prune -> rename repair -> disconnect
#
# Operates in a temp dir; never modifies the checked-in fixtures.
# Set KEEP=1 to preserve the workdir for inspection.

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

COVMAP_BIN="$ROOT/target/release/covmap"
if [[ ! -x "$COVMAP_BIN" ]]; then
  echo "==> building covmap (release)"
  ( cd "$ROOT" && cargo build --release >/dev/null )
fi

if [[ "${KEEP:-0}" == "1" ]]; then
  WORK="$HERE/_out"
  rm -rf "$WORK"
  mkdir -p "$WORK"
else
  WORK="$(mktemp -d)"
  trap 'rm -rf "$WORK"' EXIT
fi
cp -R "$HERE/spec" "$HERE/impl_rs" "$WORK/"

hr() { printf "\n===== %s =====\n" "$1"; }

# Each "repo" gets its own .covmap/.
hr "init repo A (spec)"
( cd "$WORK/spec" && "$COVMAP_BIN" init )
hr "init repo B (impl_rs)"
( cd "$WORK/impl_rs" && "$COVMAP_BIN" init )

hr "cover both (one whole-file unknown segment per file)"
( cd "$WORK/spec" && "$COVMAP_BIN" cover )
( cd "$WORK/impl_rs" && "$COVMAP_BIN" cover )

hr "status (spec) — should report drift: clean"
( cd "$WORK/spec" && "$COVMAP_BIN" status )

hr "ls -l (spec HEAD): two whole-file segments"
( cd "$WORK/spec" && "$COVMAP_BIN" ls -l )

# Refine widget.md by splitting at heading boundaries (lines 6, 10, 14, 18, 22, 26).
# After each split the bottom segment is the one that needs to be split next.
hr "refine widget.md by splitting at heading lines"
WIDGET_HASH=$( cd "$WORK/spec" && "$COVMAP_BIN" ls main | grep -F 'widget.md' | head -1 | cut -f1 )
echo "  starting hash: $WIDGET_HASH"

# Helper: split the segment of widget.md that currently contains <line>
# at <line>, labelling the top + bottom.
split_at() {
  local cwd="$1" line="$2" lt="$3" lb="$4"
  local h
  # find the hash whose [a,b] range contains <line>
  # $3 is "file:a..b" where a/b are line numbers (line-level segments
  # have no ":col"). Parse the file before the first ':' and the line
  # range around "..".
  h=$( cd "$cwd" && "$COVMAP_BIN" ls -l | awk -F'\t' -v l="$line" '
    {
      loc=$3
      ci=index(loc, ":")
      file=substr(loc, 1, ci-1)
      range=substr(loc, ci+1)
      di=index(range, "..")
      a=substr(range, 1, di-1) + 0
      b=substr(range, di+2) + 0
      if (file=="widget.md" && a<=l && l<=b) { print $1; exit }
    }')
  ( cd "$cwd" && "$COVMAP_BIN" split "main:$h" "$line" "$lt" "$lb" )
}

split_at "$WORK/spec"  6 "preamble"    "Counter"
split_at "$WORK/spec" 10 "Counter"     "Increment"
split_at "$WORK/spec" 14 "Increment"   "Decrement"
split_at "$WORK/spec" 18 "Decrement"   "Reset"
split_at "$WORK/spec" 22 "Reset"       "Identity"
split_at "$WORK/spec" 26 "Identity"    "Status"

hr "ls -l after splits (should be 7 widget.md segments + 1 errors.md segment)"
( cd "$WORK/spec" && "$COVMAP_BIN" ls -l )

hr "merge Increment + Decrement back, then split again at line 14"
INC=$( cd "$WORK/spec" && "$COVMAP_BIN" grep -l -c main "Increment" | head -1 )
DEC=$( cd "$WORK/spec" && "$COVMAP_BIN" grep -l -c main "Decrement" | head -1 )
( cd "$WORK/spec" && "$COVMAP_BIN" merge "main:$INC" "main:$DEC" "Inc+Dec" )
( cd "$WORK/spec" && "$COVMAP_BIN" ls -l )
# split it back at line 14
H=$( cd "$WORK/spec" && "$COVMAP_BIN" grep -l -c main "Inc+Dec" | head -1 )
( cd "$WORK/spec" && "$COVMAP_BIN" split "main:$H" 14 "Increment" "Decrement" )

hr "grep 'increment' in spec"
( cd "$WORK/spec" && "$COVMAP_BIN" grep increment )

hr "connect spec/main -> impl_rs/main"
( cd "$WORK/spec" && "$COVMAP_BIN" connect main "$WORK/impl_rs:main" widget_links )

hr "link Increment + Decrement -> widget.rs"
INC=$( cd "$WORK/spec"    && "$COVMAP_BIN" grep -l -c main "### Increment" | head -1 )
DEC=$( cd "$WORK/spec"    && "$COVMAP_BIN" grep -l -c main "### Decrement" | head -1 )
RS=$(  cd "$WORK/impl_rs" && "$COVMAP_BIN" ls-segments | head -1 )
echo "  inc:  $INC"
echo "  dec:  $DEC"
echo "  impl: $RS"
# The Increment endpoint is given with a redundant `main:` covering prefix
# (naming the source covering) — link accepts and strips it, like every
# other segment-address command. Decrement uses the bare hash.
( cd "$WORK/spec" && "$COVMAP_BIN" link widget_links "main:$INC" "$RS" "fn increment" )
( cd "$WORK/spec" && "$COVMAP_BIN" link widget_links "$DEC" "$RS" "fn decrement" )

hr "re-link with a new note updates it in place (not a no-op)"
( cd "$WORK/spec" && "$COVMAP_BIN" link widget_links "$DEC" "$RS" "fn decrement (revised)" )
( cd "$WORK/spec" && "$COVMAP_BIN" ls-links widget_links | grep -q "fn decrement (revised)" \
  && echo "  note updated OK" || { echo "  FAIL: note not updated"; exit 1; } )

hr "fsck (clean)"
( cd "$WORK/spec" && "$COVMAP_BIN" fsck )

hr "label the Increment segment as 'mapped'"
( cd "$WORK/spec" && "$COVMAP_BIN" label "main:$INC" mapped )

INC_AFTER=$( cd "$WORK/spec" && "$COVMAP_BIN" rev-parse "main:$INC" )
[[ "$INC_AFTER" == "$INC" ]] \
  && echo "  hash unchanged after relabel" \
  || { echo "  FAIL: relabel changed $INC to $INC_AFTER"; exit 1; }

if BROKEN_AFTER_LABEL=$( cd "$WORK/spec" && "$COVMAP_BIN" fsck --broken-links ); then
  [[ -z "$BROKEN_AFTER_LABEL" ]] \
    && echo "  existing hash-addressed link still resolves" \
    || { echo "  FAIL: fsck succeeded but reported a broken link"; printf '%s\n' "$BROKEN_AFTER_LABEL"; exit 1; }
else
  echo "  FAIL: relabel broke a link"
  printf '%s\n' "$BROKEN_AFTER_LABEL"
  exit 1
fi

hr "rev-parse <hash-prefix>"
SHORT="${INC:0:8}"
( cd "$WORK/spec" && "$COVMAP_BIN" rev-parse "$SHORT" || true )

hr "simulate spec drift — edit Decrement section"
python3 - <<PY
from pathlib import Path
p = Path("$WORK/spec/widget.md")
t = p.read_text()
t = t.replace(
    "decreases the counter by one. The counter is allowed to go negative.",
    "decreases the counter by one, but refuses to go below zero.",
)
p.write_text(t)
PY

hr "status (should report drift: 1 file edited)"
( cd "$WORK/spec" && "$COVMAP_BIN" status )

hr "fsck --broken-links (none: relabeling preserved Increment, Decrement is only drifted)"
if BROKEN_AFTER_EDIT=$( cd "$WORK/spec" && "$COVMAP_BIN" fsck --broken-links ); then
  [[ -z "$BROKEN_AFTER_EDIT" ]] \
    && echo "  no broken links" \
    || { echo "  FAIL: fsck succeeded but reported a broken link"; printf '%s\n' "$BROKEN_AFTER_EDIT"; exit 1; }
else
  echo "  FAIL: drift alone broke a link"
  printf '%s\n' "$BROKEN_AFTER_EDIT"
  exit 1
fi

hr "recut Decrement (the old hash-addressed endpoint now breaks)"
RECUT_OUT=$( cd "$WORK/spec" && "$COVMAP_BIN" recut "main:$DEC" )
printf '  %s\n' "$RECUT_OUT"
DEC_AFTER="${RECUT_OUT##* -> }"
[[ "$DEC_AFTER" != "$DEC" ]] \
  || { echo "  FAIL: content-changing recut preserved $DEC"; exit 1; }

EXPECTED_BROKEN=$(printf 'widget_links\t%s\t%s' "$DEC" "$RS")
if BROKEN_AFTER_RECUT=$( cd "$WORK/spec" && "$COVMAP_BIN" fsck --broken-links ); then
  echo "  FAIL: expected the stale Decrement hash endpoint to be broken"
  exit 1
else
  [[ "$BROKEN_AFTER_RECUT" == "$EXPECTED_BROKEN" ]] \
    && echo "  exactly one expected broken link reported" \
    || { echo "  FAIL: unexpected broken-link report"; printf '%s\n' "$BROKEN_AFTER_RECUT"; exit 1; }
fi

hr "prune widget_links (drops the stale Decrement link)"
PRUNE_OUT=$( cd "$WORK/spec" && "$COVMAP_BIN" prune widget_links )
printf '%s\n' "$PRUNE_OUT"
printf '%s\n' "$PRUNE_OUT" | grep -qF "prune: 1 link(s) dropped, 1 kept" \
  || { echo "  FAIL: prune did not report one dropped and one kept link"; exit 1; }

LINK_COUNT=$( cd "$WORK/spec" && "$COVMAP_BIN" ls-links widget_links | wc -l | tr -d ' ' )
[[ "$LINK_COUNT" == "1" ]] \
  && echo "  one live link remains" \
  || { echo "  FAIL: expected one live link after prune, found $LINK_COUNT"; exit 1; }

hr "same-repository connection lifecycle and rename repair"
ERROR_HASH=$( cd "$WORK/spec" && "$COVMAP_BIN" ls main | grep -F 'errors.md' | cut -f1 )
IDENTITY_HASH=$( cd "$WORK/spec" && "$COVMAP_BIN" rev-parse main:widget.md:22 )
RESET_HASH=$( cd "$WORK/spec" && "$COVMAP_BIN" rev-parse main:widget.md:18 )
( cd "$WORK/spec" && "$COVMAP_BIN" name "main:$ERROR_HASH" audit-source )
( cd "$WORK/spec" && "$COVMAP_BIN" name "main:$IDENTITY_HASH" audit-target )
( cd "$WORK/spec" && "$COVMAP_BIN" name "main:$RESET_HASH" audit-target-two )
( cd "$WORK/spec" && "$COVMAP_BIN" connect main main self_audit )

STATUS_SELF=$( cd "$WORK/spec" && "$COVMAP_BIN" status )
SHOW_SELF=$( cd "$WORK/spec" && "$COVMAP_BIN" show self_audit )
printf '%s\n' "$STATUS_SELF" | grep -qF "self_audit: main -> main (self-map)" \
  || { echo "  FAIL: status did not identify the same-repository connection"; exit 1; }
printf '%s\n' "$SHOW_SELF" | grep -qF "main -> main (self-map)" \
  || { echo "  FAIL: show did not identify the same-repository connection"; exit 1; }

REPAIR_NOTE='literal $HOME, apostrophe '"'"', and `touch SHOULD_NOT_EXIST`'
SECOND_REPAIR_NOTE='fan-out second link'
( cd "$WORK/spec" && "$COVMAP_BIN" link self_audit @audit-source @audit-target "$REPAIR_NOTE" )
( cd "$WORK/spec" && "$COVMAP_BIN" link self_audit @audit-source @audit-target-two "$SECOND_REPAIR_NOTE" )

run_repair_block() {
  local output="$1"
  local script
  [[ $(printf '%s\n' "$output" | grep -c '^--- covmap repair begin ---$') == "1" \
    && $(printf '%s\n' "$output" | grep -c '^--- covmap repair end ---$') == "1" ]] \
    || { echo "  FAIL: rename did not emit exactly one repair block"; printf '%s\n' "$output"; exit 1; }
  script=$(printf '%s\n' "$output" \
    | sed -n '/^--- covmap repair begin ---$/,/^--- covmap repair end ---$/p' \
    | sed '1d;$d')
  [[ -n "$script" ]] \
    || { echo "  FAIL: rename emitted no repair block"; printf '%s\n' "$output"; exit 1; }
  ( cd "$WORK/spec" && PATH="$(dirname "$COVMAP_BIN"):$PATH" sh -eu -c "$script" )
}

SOURCE_RENAME=$( cd "$WORK/spec" && "$COVMAP_BIN" name main:@audit-source audit-source-next )
printf '%s\n' "$SOURCE_RENAME"
printf '%s\n' "$SOURCE_RENAME" | grep -qF "stored source endpoint '@audit-source' is now broken" \
  || { echo "  FAIL: source-side rename impact was not reported"; exit 1; }
run_repair_block "$SOURCE_RENAME"

TARGET_RENAME=$( cd "$WORK/spec" && "$COVMAP_BIN" name main:@audit-target audit-target-next )
printf '%s\n' "$TARGET_RENAME"
printf '%s\n' "$TARGET_RENAME" | grep -qF "stored target endpoint '@audit-target' is now broken" \
  || { echo "  FAIL: target-side rename impact was not reported"; exit 1; }
run_repair_block "$TARGET_RENAME"

[[ ! -e "$WORK/spec/SHOULD_NOT_EXIST" ]] \
  || { echo "  FAIL: repair note syntax was executed by the shell"; exit 1; }
ACTUAL_REPAIR_NOTE=$( cd "$WORK/spec" && "$COVMAP_BIN" ls-links self_audit \
  | grep $'^@audit-source-next\\t@audit-target-next\\t' | cut -f3- )
[[ "$ACTUAL_REPAIR_NOTE" == "$REPAIR_NOTE" ]] \
  || { echo "  FAIL: repair changed the link note"; exit 1; }
ACTUAL_SECOND_NOTE=$( cd "$WORK/spec" && "$COVMAP_BIN" ls-links self_audit \
  | grep $'^@audit-source-next\\t@audit-target-two\\t' | cut -f3- )
[[ "$ACTUAL_SECOND_NOTE" == "$SECOND_REPAIR_NOTE" ]] \
  || { echo "  FAIL: fan-out repair changed the second link note"; exit 1; }
( cd "$WORK/spec" && "$COVMAP_BIN" fsck )

hr "unlink accepts exact @name endpoints"
( cd "$WORK/spec" && "$COVMAP_BIN" unlink self_audit @audit-source-next @audit-target-next )
[[ $( cd "$WORK/spec" && "$COVMAP_BIN" ls-links self_audit | wc -l | tr -d ' ' ) == "1" ]] \
  || { echo "  FAIL: name-addressed unlink did not remove the link"; exit 1; }
( cd "$WORK/spec" && "$COVMAP_BIN" link self_audit @audit-source-next @audit-target-next "$REPAIR_NOTE" )

hr "disconnect removes only the connection ref and warns about the default report"
( cd "$WORK/spec" && "$COVMAP_BIN" html --connection self_audit )
OBJECTS_BEFORE_DISCONNECT=$(find "$WORK/spec/.covmap/objects" -type f | wc -l | tr -d ' ')
COVERINGS_BEFORE_DISCONNECT=$(find "$WORK/spec/.covmap/coverings" -type f | wc -l | tr -d ' ')
DISCONNECT_OUT=$( cd "$WORK/spec" && "$COVMAP_BIN" disconnect self_audit 2>&1 )
printf '%s\n' "$DISCONNECT_OUT"
printf '%s\n' "$DISCONNECT_OUT" | grep -qF "removed 2 link(s) from main -> main (self-map)" \
  || { echo "  FAIL: disconnect summary omitted link count or coverings"; exit 1; }
printf '%s\n' "$DISCONNECT_OUT" | grep -qF ".covmap/report.html may be stale" \
  || { echo "  FAIL: disconnect did not warn about the default report"; exit 1; }
[[ ! -e "$WORK/spec/.covmap/connections/self_audit" ]] \
  || { echo "  FAIL: disconnect left the connection ref behind"; exit 1; }
[[ $(find "$WORK/spec/.covmap/objects" -type f | wc -l | tr -d ' ') == "$OBJECTS_BEFORE_DISCONNECT" ]] \
  || { echo "  FAIL: disconnect changed the object store"; exit 1; }
[[ $(find "$WORK/spec/.covmap/coverings" -type f | wc -l | tr -d ' ') == "$COVERINGS_BEFORE_DISCONNECT" ]] \
  || { echo "  FAIL: disconnect changed a covering"; exit 1; }
if ( cd "$WORK/spec" && "$COVMAP_BIN" show self_audit >/dev/null 2>&1 ); then
  echo "  FAIL: disconnected connection is still showable"
  exit 1
fi

hr "disconnect refuses to delete a malformed connection ref"
printf '# source_covering=main\nmalformed-link\n' > "$WORK/spec/.covmap/connections/malformed"
if ( cd "$WORK/spec" && "$COVMAP_BIN" disconnect malformed >/dev/null 2>&1 ); then
  echo "  FAIL: malformed connection was silently deleted"
  exit 1
fi
[[ -e "$WORK/spec/.covmap/connections/malformed" ]] \
  || { echo "  FAIL: malformed connection ref was deleted"; exit 1; }
rm "$WORK/spec/.covmap/connections/malformed"

hr "final fsck"
( cd "$WORK/spec" && "$COVMAP_BIN" fsck )
( cd "$WORK/impl_rs" && "$COVMAP_BIN" fsck )

hr "render html report"
( cd "$WORK/spec" && "$COVMAP_BIN" html )
ls -la "$WORK/spec/.covmap/report.html"
echo "  size: $(wc -c < "$WORK/spec/.covmap/report.html") bytes"

hr "render html for a specific connection (--connection selector)"
( cd "$WORK/spec" && "$COVMAP_BIN" html --connection widget_links -o "$WORK/spec/.covmap/widget_links.html" )
grep -q "widget_links" "$WORK/spec/.covmap/widget_links.html" \
  && echo "  OK  connection-scoped report names the connection" \
  || { echo "  FAIL: connection missing from --connection report"; exit 1; }

echo
echo "SMOKE TEST OK — artifacts under: $WORK"
if [[ "${KEEP:-0}" == "1" ]]; then
  echo "  spec repo:    $WORK/spec/.covmap/"
  echo "  impl repo:    $WORK/impl_rs/.covmap/"
else
  echo "(temp dir will be removed at exit; re-run with KEEP=1 ./run.sh to preserve it)"
fi
