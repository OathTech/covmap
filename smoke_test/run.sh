#!/usr/bin/env bash
# Smoke test for covmap. Exercises:
#   init -> cover -> split -> merge -> connect -> link -> fsck
#   -> label -> drift after edit -> prune
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

hr "fsck --broken-links (one broken: the Increment hash moved when we relabeled to 'mapped')"
( cd "$WORK/spec" && "$COVMAP_BIN" fsck --broken-links ) || true

hr "prune widget_links (drops links whose endpoints disappeared from a covering)"
( cd "$WORK/spec" && "$COVMAP_BIN" prune widget_links )

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
