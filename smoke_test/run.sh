#!/usr/bin/env bash
# Smoke test for covmap. Exercises:
#   init -> cover -> atomic multi-cut -> merge -> connect -> link -> fsck
#   -> review surfaces -> label -> drift -> prune -> rename repair -> disconnect
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
object_file() {
  local repo="$1" hash="$2"
  printf '%s/.covmap/objects/%s/%s' "$repo" "${hash:0:2}" "${hash:2}"
}

# Each "repo" gets its own .covmap/.
hr "init repo A (spec)"
( cd "$WORK/spec" && "$COVMAP_BIN" init )
hr "init repo B (impl_rs)"
( cd "$WORK/impl_rs" && "$COVMAP_BIN" init )

hr "cover both (one whole-file unknown segment per file)"
( cd "$WORK/spec" && "$COVMAP_BIN" cover )
( cd "$WORK/impl_rs" && "$COVMAP_BIN" cover )
[[ $(cat "$WORK/spec/.covmap/version") == "4" ]] \
  || { echo "  FAIL: init did not create format 4"; exit 1; }
cmp -s "$WORK/spec/.covmap/.gitignore" <(printf '%s' \
"# Local and reconstructible covmap state.
# Commit selection policy, coverings, connections, and covmap metadata.
/local/
/objects/
/iterators/
/*.html
\#covmap-tmp-*.tmp
") || { echo "  FAIL: generated .covmap/.gitignore differs from policy"; exit 1; }

hr "tmp-suffixed covering refs remain Git-trackable"
TMP_POLICY="$WORK/tmp_policy"
mkdir -p "$TMP_POLICY"
printf 'tracked\n' > "$TMP_POLICY/source.txt"
( cd "$TMP_POLICY" && "$COVMAP_BIN" init >/dev/null \
  && printf '/source.txt\n' > .covmap/include \
  && "$COVMAP_BIN" cover release.tmp >/dev/null \
  && : > '.covmap/coverings/#covmap-tmp-release.tmp.tmp' \
  && git init -q \
  && git add .covmap )
TMP_STAGED=$( cd "$TMP_POLICY" && git diff --cached --name-only )
printf '%s\n' "$TMP_STAGED" | grep -qxF '.covmap/coverings/release.tmp' \
  || { echo "  FAIL: tmp-suffixed covering ref was not staged"; exit 1; }
if printf '%s\n' "$TMP_STAGED" | grep -qF '#covmap-tmp-release.tmp.tmp'; then
  echo "  FAIL: reserved atomic temp file was staged"
  exit 1
fi

hr "default Git publication policy stages refs but not local state"
( cd "$WORK/spec" && mkdir -p .covmap/objects/aa .covmap/iterators .covmap/local \
  && : > .covmap/objects/aa/.partial.tmp \
  && : > .covmap/iterators/session \
  && : > .covmap/local/peers \
  && : > .covmap/report.html )
( cd "$WORK/spec" && git init -q && git add .covmap )
STAGED=$( cd "$WORK/spec" && git diff --cached --name-only )
for path in .covmap/.gitignore .covmap/HEAD .covmap/version .covmap/coverings/main; do
  printf '%s\n' "$STAGED" | grep -qxF "$path" \
    || { echo "  FAIL: durable artifact '$path' was not staged"; exit 1; }
done
if printf '%s\n' "$STAGED" | grep -qE '^\.covmap/(objects|iterators|local)/'; then
  echo "  FAIL: local covmap state was staged"
  exit 1
fi
if ( cd "$WORK/spec" && git ls-files .covmap/report.html .covmap/objects/aa/.partial.tmp \
  .covmap/iterators/session .covmap/local/peers | grep -q . ); then
  echo "  FAIL: generated report, nested temp, iterator, or binding was staged"
  exit 1
fi
rm -f "$WORK/spec/.covmap/iterators/session"
ERROR_HASH=$( cd "$WORK/spec" && "$COVMAP_BIN" ls-segments -l main | awk -F '\t' '$2 == "errors.md" { print $1 }' )
WIDGET_HASH=$( cd "$WORK/spec" && "$COVMAP_BIN" ls-segments -l main | awk -F '\t' '$2 == "widget.md" { print $1 }' )

hr "status (spec) — should report drift: clean"
( cd "$WORK/spec" && "$COVMAP_BIN" status )

hr "missing live objects are reconstructible cache misses"
rm -rf "$WORK/spec/.covmap/objects"/*
( cd "$WORK/spec" && "$COVMAP_BIN" fsck )
( cd "$WORK/spec" && "$COVMAP_BIN" fsck --offline )
[[ $(find "$WORK/spec/.covmap/objects" -type f | wc -l | tr -d ' ') == "0" ]] \
  || { echo "  FAIL: ordinary fsck unexpectedly populated the cache"; exit 1; }
( cd "$WORK/spec" && "$COVMAP_BIN" rev-parse "${ERROR_HASH:0:8}" | grep -qF "$ERROR_HASH" )
[[ $(find "$WORK/spec/.covmap/objects" -type f | wc -l | tr -d ' ') == "0" ]] \
  || { echo "  FAIL: live hash resolution unexpectedly populated the cache"; exit 1; }
( cd "$WORK/spec" && "$COVMAP_BIN" show "main:$ERROR_HASH" --content | grep -qF "# Errors" )
[[ -f "$(object_file "$WORK/spec" "$ERROR_HASH")" ]] \
  || { echo "  FAIL: show --content did not warm its live object"; exit 1; }
rm -rf "$WORK/spec/.covmap/objects"/*
( cd "$WORK/spec" && "$COVMAP_BIN" cat-segment "${ERROR_HASH:0:8}" | grep -qF "# Errors" )
[[ -f "$(object_file "$WORK/spec" "$ERROR_HASH")" ]] \
  || { echo "  FAIL: cat-segment did not reconstruct a live hash"; exit 1; }
rm -rf "$WORK/spec/.covmap/objects"/*
( cd "$WORK/spec" && "$COVMAP_BIN" grep Counter >/dev/null )
[[ $(find "$WORK/spec/.covmap/objects" -type f | wc -l | tr -d ' ') == "2" ]] \
  || { echo "  FAIL: grep did not reconstruct live covering content"; exit 1; }
rm -rf "$WORK/spec/.covmap/objects"/*
( cd "$WORK/spec" && "$COVMAP_BIN" ls -l main >/dev/null )
[[ $(find "$WORK/spec/.covmap/objects" -type f | wc -l | tr -d ' ') == "2" ]] \
  || { echo "  FAIL: ls did not opportunistically warm live objects"; exit 1; }
rm -rf "$WORK/spec/.covmap/objects"/*
( cd "$WORK/spec" && "$COVMAP_BIN" status | grep -qF "drift: clean" )
[[ $(find "$WORK/spec/.covmap/objects" -type f | wc -l | tr -d ' ') == "2" ]] \
  || { echo "  FAIL: status did not opportunistically warm live objects"; exit 1; }
rm -rf "$WORK/spec/.covmap/objects"/*
# Directory mode bits do not make a cache unwritable when CI runs as root.
# A regular file at the cache path makes nested publication fail portably.
mv "$WORK/spec/.covmap/objects" "$WORK/spec/.covmap/objects.available"
: > "$WORK/spec/.covmap/objects"
( cd "$WORK/spec" && "$COVMAP_BIN" status | grep -qF "drift: clean" )
[[ -f "$WORK/spec/.covmap/objects" ]] \
  || { echo "  FAIL: best-effort warming replaced the unavailable cache"; exit 1; }
rm "$WORK/spec/.covmap/objects"
mv "$WORK/spec/.covmap/objects.available" "$WORK/spec/.covmap/objects"
[[ $(find "$WORK/spec/.covmap/objects" -type f | wc -l | tr -d ' ') == "0" ]] \
  || { echo "  FAIL: unavailable-cache warming unexpectedly wrote objects"; exit 1; }
( cd "$WORK/spec" && "$COVMAP_BIN" iter cache_probe new main )
[[ $(find "$WORK/spec/.covmap/objects" -type f | wc -l | tr -d ' ') == "2" ]] \
  || { echo "  FAIL: iter new did not retain both missing task objects"; exit 1; }
( cd "$WORK/spec" && "$COVMAP_BIN" iter cache_probe rm )
rm -rf "$WORK/spec/.covmap/objects"/*
( cd "$WORK/spec" && "$COVMAP_BIN" fsck --offline --hydrate )
[[ $(find "$WORK/spec/.covmap/objects" -type f | wc -l | tr -d ' ') == "2" ]] \
  || { echo "  FAIL: offline fsck did not hydrate both live objects"; exit 1; }
rm -rf "$WORK/spec/.covmap/objects"/*
( cd "$WORK/spec" && "$COVMAP_BIN" hydrate main )
[[ $(find "$WORK/spec/.covmap/objects" -type f | wc -l | tr -d ' ') == "2" ]] \
  || { echo "  FAIL: hydrate did not restore both live objects"; exit 1; }
mv "$WORK/spec/errors.md" "$WORK/spec/errors.md.unavailable"
if MISSING_SOURCE_OUT=$( cd "$WORK/spec" && "$COVMAP_BIN" fsck 2>&1 ); then
  mv "$WORK/spec/errors.md.unavailable" "$WORK/spec/errors.md"
  echo "  FAIL: fsck accepted unavailable covered source backed by a valid cache"
  exit 1
fi
mv "$WORK/spec/errors.md.unavailable" "$WORK/spec/errors.md"
printf '%s\n' "$MISSING_SOURCE_OUT" | grep -qF "cannot read source 'errors.md'" \
  || { echo "  FAIL: unavailable-source diagnostic was not explicit"; exit 1; }

hr "ls -l (spec HEAD): two whole-file segments"
( cd "$WORK/spec" && "$COVMAP_BIN" ls -l )

# Refine widget.md atomically at every heading boundary. Cuts are deliberately
# unsorted; output and covering order must still follow file position.
hr "refine widget.md with one atomic multi-cut"
echo "  starting hash: $WIDGET_HASH"
rm -f "$(object_file "$WORK/spec" "$WIDGET_HASH")"
( cd "$WORK/spec" && "$COVMAP_BIN" split main:widget.md 26 10 18 6 22 14 )

# Geometry and status metadata compose explicitly.
( cd "$WORK/spec" && "$COVMAP_BIN" label main:widget.md:1  preamble )
( cd "$WORK/spec" && "$COVMAP_BIN" label main:widget.md:6  Counter )
( cd "$WORK/spec" && "$COVMAP_BIN" label main:widget.md:10 Increment )
( cd "$WORK/spec" && "$COVMAP_BIN" label main:widget.md:14 Decrement )
( cd "$WORK/spec" && "$COVMAP_BIN" label main:widget.md:18 Reset )
( cd "$WORK/spec" && "$COVMAP_BIN" label main:widget.md:22 Identity )
( cd "$WORK/spec" && "$COVMAP_BIN" label main:widget.md:26 Status )

hr "file-addressed existing cuts are idempotent and do not rewrite the ref"
REF_INODE_BEFORE=$(ls -i "$WORK/spec/.covmap/coverings/main" | awk '{print $1}')
IDEMPOTENT_OUT=$( cd "$WORK/spec" && "$COVMAP_BIN" split main:widget.md 10 14 22 )
printf '%s\n' "$IDEMPOTENT_OUT"
REF_INODE_AFTER=$(ls -i "$WORK/spec/.covmap/coverings/main" | awk '{print $1}')
[[ "$REF_INODE_BEFORE" == "$REF_INODE_AFTER" ]] \
  || { echo "  FAIL: all-existing multi-cut rewrote the covering ref"; exit 1; }
[[ $(printf '%s\n' "$IDEMPOTENT_OUT" | grep -c "already satisfied") == "3" ]] \
  || { echo "  FAIL: existing cuts were not reported separately"; exit 1; }

hr "legacy split labels are rejected atomically with a metadata hint"
REF_HASH_BEFORE=$(shasum "$WORK/spec/.covmap/coverings/main" | awk '{print $1}')
if LEGACY_OUT=$( cd "$WORK/spec" && "$COVMAP_BIN" split main:widget.md 12 FormerTop 2>&1 ); then
  echo "  FAIL: label-bearing legacy split unexpectedly succeeded"
  exit 1
fi
REF_HASH_AFTER=$(shasum "$WORK/spec/.covmap/coverings/main" | awk '{print $1}')
[[ "$REF_HASH_BEFORE" == "$REF_HASH_AFTER" ]] \
  || { echo "  FAIL: rejected multi-cut changed the covering ref"; exit 1; }
printf '%s\n' "$LEGACY_OUT" | grep -qF "Apply metadata afterward" \
  || { echo "  FAIL: legacy-label error omitted metadata composition guidance"; exit 1; }
[[ $(cat "$WORK/spec/.covmap/version") == "4" ]] \
  || { echo "  FAIL: atomic multi-cut changed the repository format"; exit 1; }

hr "ls -l after splits (should be 7 widget.md segments + 1 errors.md segment)"
( cd "$WORK/spec" && "$COVMAP_BIN" ls -l )

hr "merge Increment + Decrement back, then split again at line 14"
INC=$( cd "$WORK/spec" && "$COVMAP_BIN" grep -l -c main "Increment" | head -1 )
DEC=$( cd "$WORK/spec" && "$COVMAP_BIN" grep -l -c main "Decrement" | head -1 )
rm -f "$(object_file "$WORK/spec" "$INC")" "$(object_file "$WORK/spec" "$DEC")"
( cd "$WORK/spec" && "$COVMAP_BIN" merge "main:$INC" "main:$DEC" "Inc+Dec" )
( cd "$WORK/spec" && "$COVMAP_BIN" ls -l )
# split it back at line 14
H=$( cd "$WORK/spec" && "$COVMAP_BIN" grep -l -c main "Inc+Dec" | head -1 )
( cd "$WORK/spec" && "$COVMAP_BIN" split "main:$H" 14 )
( cd "$WORK/spec" && "$COVMAP_BIN" label main:widget.md:10 Increment )
( cd "$WORK/spec" && "$COVMAP_BIN" label main:widget.md:14 Decrement )

hr "grep 'increment' in spec"
( cd "$WORK/spec" && "$COVMAP_BIN" grep increment )

hr "connect spec/main -> impl_rs/main"
( cd "$WORK/spec" && "$COVMAP_BIN" peer bind impl ../impl_rs )
( cd "$WORK/spec" && "$COVMAP_BIN" peer ls | grep -q $'^impl\tbound\t' )
( cd "$WORK/spec" && "$COVMAP_BIN" connect main impl:main widget_links )
grep -qF "# target_peer=impl" "$WORK/spec/.covmap/connections/widget_links" \
  || { echo "  FAIL: connection did not store the peer alias"; exit 1; }
if grep -qF "$WORK/impl_rs" "$WORK/spec/.covmap/connections/widget_links"; then
  echo "  FAIL: connection leaked an absolute checkout path"
  exit 1
fi

hr "link Increment + Decrement -> widget.rs"
INC=$( cd "$WORK/spec"    && "$COVMAP_BIN" grep -l -c main "### Increment" | head -1 )
DEC=$( cd "$WORK/spec"    && "$COVMAP_BIN" grep -l -c main "### Decrement" | head -1 )
RS=$(  cd "$WORK/impl_rs" && "$COVMAP_BIN" ls-segments | head -1 )
INCREMENT_NOTE="The increment contract maps to the shared widget implementation segment and keeps this rationale readable during connection review."
echo "  inc:  $INC"
echo "  dec:  $DEC"
echo "  impl: $RS"
# The Increment endpoint is given with a redundant `main:` covering prefix
# (naming the source covering) — link accepts and strips it, like every
# other segment-address command. Decrement uses the bare hash.
( cd "$WORK/spec" && "$COVMAP_BIN" link widget_links "main:$INC" "$RS" "$INCREMENT_NOTE" )
# Name the target after the first link. The first stored endpoint remains the
# resolving hash; the second is stored as @impl-widget. Degree must combine
# both spellings into one target node.
( cd "$WORK/impl_rs" && "$COVMAP_BIN" name "main:$RS" impl-widget )
( cd "$WORK/spec" && "$COVMAP_BIN" link widget_links "$DEC" @impl-widget "fn decrement" )

hr "unbound peer degrades inspection but blocks target mutation"
( cd "$WORK/spec" && "$COVMAP_BIN" connect main impl:main disposable_peer_ref )
( cd "$WORK/spec" && "$COVMAP_BIN" peer unbind impl )
UNBOUND_STATUS=$( cd "$WORK/spec" && "$COVMAP_BIN" status )
printf '%s\n' "$UNBOUND_STATUS" | grep -qF "unavailable: peer 'impl' is not bound"
printf '%s\n' "$UNBOUND_STATUS" | grep -qF "0 source-broken, target unchecked"
( cd "$WORK/spec" && "$COVMAP_BIN" show widget_links | grep -qF "target unchecked" )
( cd "$WORK/spec" && "$COVMAP_BIN" show "main:$INC" | grep -qF "(unchecked)" )
( cd "$WORK/spec" && "$COVMAP_BIN" ls-links widget_links >/dev/null )
RESOLVE_ERR="$WORK/resolve.err"
( cd "$WORK/spec" && "$COVMAP_BIN" ls-links --resolve widget_links 2>"$RESOLVE_ERR" \
  | awk -F '\t' 'NF == 5 && $3 == "" && $4 == "" { ok++ } END { exit ok == 2 ? 0 : 1 }' )
grep -qF "peer 'impl' is unavailable" "$RESOLVE_ERR" \
  || { echo "  FAIL: resolved plumbing did not warn about the unbound peer"; exit 1; }
( cd "$WORK/spec" && "$COVMAP_BIN" disconnect disposable_peer_ref )
[[ ! -e "$WORK/spec/.covmap/connections/disposable_peer_ref" ]] \
  || { echo "  FAIL: disconnect required an external peer binding"; exit 1; }
if ( cd "$WORK/spec" && "$COVMAP_BIN" link widget_links "$INC" "$RS" >/dev/null 2>&1 ); then
  echo "  FAIL: link accepted an unbound target peer"
  exit 1
fi
( cd "$WORK/spec" && "$COVMAP_BIN" peer bind impl ../impl_rs )

hr "portable connection survives clones at different checkout paths"
( cd "$WORK/spec" && git add errors.md widget.md .covmap \
  && git -c user.name=covmap -c user.email=covmap@example.invalid commit -qm "fixture" )
( cd "$WORK/impl_rs" && git init -q && git add widget.rs .covmap \
  && git -c user.name=covmap -c user.email=covmap@example.invalid commit -qm "fixture" )
mkdir -p "$WORK/review/deps"
git clone -q "$WORK/spec" "$WORK/review/spec-clone"
git clone -q "$WORK/impl_rs" "$WORK/review/deps/implementation"
cmp "$WORK/spec/.covmap/connections/widget_links" \
    "$WORK/review/spec-clone/.covmap/connections/widget_links"
[[ ! -e "$WORK/review/spec-clone/.covmap/local/peers" ]] \
  || { echo "  FAIL: clone unexpectedly contained local peer bindings"; exit 1; }
[[ $(find "$WORK/review/spec-clone/.covmap/objects" -type f 2>/dev/null | wc -l | tr -d ' ') == "0" ]] \
  || { echo "  FAIL: clone unexpectedly contained cached objects"; exit 1; }
( cd "$WORK/review/spec-clone" && "$COVMAP_BIN" fsck --offline --hydrate )
( cd "$WORK/review/spec-clone" && "$COVMAP_BIN" peer bind impl ../deps/implementation )
( cd "$WORK/review/spec-clone" && "$COVMAP_BIN" fsck )
cmp "$WORK/spec/.covmap/connections/widget_links" \
    "$WORK/review/spec-clone/.covmap/connections/widget_links"
[[ -z $(cd "$WORK/review/spec-clone" && git status --short) ]] \
  || { echo "  FAIL: hydration or peer binding dirtied the clone"; exit 1; }

hr "re-link with a new note updates it in place (not a no-op)"
( cd "$WORK/spec" && "$COVMAP_BIN" link widget_links "$DEC" "$RS" "fn decrement (revised)" )
( cd "$WORK/spec" && "$COVMAP_BIN" ls-links widget_links | grep -q "fn decrement (revised)" \
  && echo "  note updated OK" || { echo "  FAIL: note not updated"; exit 1; } )

hr "review surfaces: region union, resolved degree, wrapped notes, stable plumbing"
SHOW_HELP=$( "$COVMAP_BIN" show --help )
TOP_HELP=$( "$COVMAP_BIN" help 2>&1 )
printf '%s\n' "$SHOW_HELP" | grep -qF "segment, covering, region, link, or connection" \
  && printf '%s\n' "$TOP_HELP" | grep -qF "segment, covering, region, link, or connection" \
  || { echo "  FAIL: focused and top-level show help disagree"; exit 1; }
LS_HELP=$( "$COVMAP_BIN" ls --help )
printf '%s\n' "$LS_HELP" | grep -qF "list segments, optionally narrowed to regions" \
  && printf '%s\n' "$TOP_HELP" | grep -qF "list segments, optionally narrowed to regions" \
  || { echo "  FAIL: focused and top-level ls help disagree"; exit 1; }

MULTI_LS=$( cd "$WORK/spec" && "$COVMAP_BIN" ls -l main errors.md widget.md:10..20 widget.md:14..20 )
printf '%s\n' "$MULTI_LS"
[[ $(printf '%s\n' "$MULTI_LS" | wc -l | tr -d ' ') == "4" ]] \
  || { echo "  FAIL: overlapping multi-region ls duplicated or omitted segments"; exit 1; }
[[ $(printf '%s\n' "$MULTI_LS" | grep -c 'errors.md') == "1" \
  && $(printf '%s\n' "$MULTI_LS" | grep -c 'widget.md') == "3" ]] \
  && ! printf '%s\n' "$MULTI_LS" | grep -q 'widget.md:1..5' \
  || { echo "  FAIL: multi-region ls did not restrict the union"; exit 1; }

REGION_LINKED=$( cd "$WORK/spec" && "$COVMAP_BIN" ls -l main widget.md:10..13 --linked )
[[ $(printf '%s\n' "$REGION_LINKED" | wc -l | tr -d ' ') == "1" \
  && "$REGION_LINKED" == *$'\tIncrement\twidget.md:10..13\t'* ]] \
  || { echo "  FAIL: linked filter did not intersect the region union"; exit 1; }
REGION_LABEL=$( cd "$WORK/spec" && "$COVMAP_BIN" ls -l main widget.md:14..17 --label Increment )
[[ -z "$REGION_LABEL" ]] \
  || { echo "  FAIL: label filter did not intersect the region union"; exit 1; }
IMPLICIT_LINKED=$( cd "$WORK/spec" && "$COVMAP_BIN" ls -l --linked )
[[ "$IMPLICIT_LINKED" == "$( cd "$WORK/spec" && "$COVMAP_BIN" ls -l main --linked )" ]] \
  || { echo "  FAIL: implicit HEAD changed linked-filter results"; exit 1; }

if BAD_REGION=$( cd "$WORK/spec" && "$COVMAP_BIN" ls -l main widget.md:20..10 2>&1 ); then
  echo "  FAIL: reversed region unexpectedly succeeded"
  exit 1
fi
printf '%s\n' "$BAD_REGION" | grep -qF "invalid region 'widget.md:20..10'" \
  || { echo "  FAIL: invalid region did not name the offending token"; exit 1; }

if BAD_RANGE=$( cd "$WORK/spec" && "$COVMAP_BIN" ls -l main widget.md:10 2>&1 ); then
  echo "  FAIL: single-line region unexpectedly succeeded"
  exit 1
fi
printf '%s\n' "$BAD_RANGE" | grep -qF "a line range must use a..b" \
  || { echo "  FAIL: single-line region error misdiagnosed the range"; exit 1; }

if BAD_COVERING=$( cd "$WORK/spec" && "$COVMAP_BIN" ls -l typo 2>&1 ); then
  echo "  FAIL: unknown covering/region unexpectedly succeeded"
  exit 1
fi
printf '%s\n' "$BAD_COVERING" | grep -qF "'typo' is neither an existing covering nor a covered file" \
  || { echo "  FAIL: covering typo did not explain the ambiguity"; exit 1; }

SHOW_WIDGET_LINKS=$( cd "$WORK/spec" && "$COVMAP_BIN" show widget_links )
printf '%s\n' "$SHOW_WIDGET_LINKS"
printf '%s\n' "$SHOW_WIDGET_LINKS" | grep -qF "degree max-out 1, max-in 2" \
  || { echo "  FAIL: connection degree did not combine hash and @name target spellings"; exit 1; }
printf '%s\n' "$SHOW_WIDGET_LINKS" | grep -qF "2 <- unknown widget.rs:1..28" \
  || { echo "  FAIL: resolved fan-in hub was not reported"; exit 1; }
printf '%s\n' "$SHOW_WIDGET_LINKS" | grep -qF "    The increment contract maps to the shared widget implementation segment" \
  || { echo "  FAIL: long link note was not wrapped beneath the endpoint pair"; exit 1; }

RESOLVED_PLUMBING=$( cd "$WORK/spec" && "$COVMAP_BIN" ls-links --resolve widget_links )
[[ $(printf '%s\n' "$RESOLVED_PLUMBING" | wc -l | tr -d ' ') == "2" ]] \
  || { echo "  FAIL: resolved plumbing changed row cardinality"; exit 1; }
printf '%s\n' "$RESOLVED_PLUMBING" | grep -qF "$INCREMENT_NOTE" \
  || { echo "  FAIL: resolved plumbing changed the long note"; exit 1; }
STATUS_REVIEW=$( cd "$WORK/spec" && "$COVMAP_BIN" status )
if printf '%s\n' "$STATUS_REVIEW" | grep -q "degree"; then
  echo "  FAIL: status gained degree detail"
  exit 1
fi

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
STATUS_AFTER_EDIT=$( cd "$WORK/spec" && "$COVMAP_BIN" status )
printf '%s\n' "$STATUS_AFTER_EDIT"
grep -Fq "    edited: widget.md" <<<"$STATUS_AFTER_EDIT" \
  || { echo "  FAIL: status did not name the edited file"; exit 1; }

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

hr "iter new refuses a drifted cache miss with recovery guidance"
rm -f "$(object_file "$WORK/spec" "$DEC")"
if CAT_DRIFT=$( cd "$WORK/spec" && "$COVMAP_BIN" cat-segment "${DEC:0:8}" 2>&1 ); then
  echo "  FAIL: cat-segment returned drifted source for a missing live object"
  exit 1
fi
printf '%s\n' "$CAT_DRIFT" | grep -qF "resolved but recorded bytes are unavailable" \
  || { echo "  FAIL: cat-segment did not distinguish a live hash from missing bytes"; exit 1; }
if ITER_DRIFT=$( cd "$WORK/spec" && "$COVMAP_BIN" iter drift_probe new main --label Decrement 2>&1 ); then
  echo "  FAIL: iter new froze a task whose recorded bytes were unavailable"
  exit 1
fi
printf '%s\n' "$ITER_DRIFT" | grep -qF "recorded bytes unavailable" \
  && printf '%s\n' "$ITER_DRIFT" | grep -qF 'run `covmap hydrate`' \
  || { echo "  FAIL: iter new did not explain missing-recorded-byte recovery"; exit 1; }
[[ ! -e "$WORK/spec/.covmap/iterators/drift_probe" ]] \
  || { echo "  FAIL: failed iter new left a frozen worklist"; exit 1; }

hr "recut Decrement (the old hash-addressed endpoint now breaks)"
RECUT_OUT=$( cd "$WORK/spec" && "$COVMAP_BIN" recut "main:$DEC" )
printf '  %s\n' "$RECUT_OUT"
DEC_AFTER="${RECUT_OUT##* -> }"
[[ "$DEC_AFTER" != "$DEC" ]] \
  || { echo "  FAIL: content-changing recut preserved $DEC"; exit 1; }

EXPECTED_BROKEN=$(printf 'widget_links\t%s\t%s' "$DEC" "@impl-widget")
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

hr "subdivision diagnoses name narrowing and broken hash endpoints"
NAME_SPLIT=$( cd "$WORK/spec" && "$COVMAP_BIN" split main:@audit-source 8 )
printf '%s\n' "$NAME_SPLIT"
printf '%s\n' "$NAME_SPLIT" | grep -qF "name endpoint '@audit-source' will narrow from 1..10 to 1..7" \
  || { echo "  FAIL: named subdivision did not report its narrowed range"; exit 1; }
[[ $(printf '%s\n' "$NAME_SPLIT" | grep -cF "name endpoint '@audit-source' will narrow") == "1" ]] \
  || { echo "  FAIL: named subdivision repeated an identical fan-out warning"; exit 1; }
printf '%s\n' "$NAME_SPLIT" | grep -qF "inbound connections stored in another repository cannot be discovered" \
  || { echo "  FAIL: subdivision omitted the inbound-discovery limit"; exit 1; }

STATUS_HASH=$( cd "$WORK/spec" && "$COVMAP_BIN" rev-parse main:widget.md:26 )
( cd "$WORK/spec" && "$COVMAP_BIN" link self_audit "$STATUS_HASH" @audit-target-two "hash subdivision" )
HASH_SPLIT=$( cd "$WORK/spec" && "$COVMAP_BIN" split main:widget.md:26 28 )
printf '%s\n' "$HASH_SPLIT"
printf '%s\n' "$HASH_SPLIT" | grep -qF "hash endpoint '$STATUS_HASH' will become broken" \
  || { echo "  FAIL: hash-addressed subdivision was not diagnosed"; exit 1; }
PRUNE_SPLIT=$( cd "$WORK/spec" && "$COVMAP_BIN" prune self_audit )
printf '%s\n' "$PRUNE_SPLIT" | grep -qF "prune: 1 link(s) dropped, 2 kept" \
  || { echo "  FAIL: subdivision did not produce exactly one broken hash link"; exit 1; }

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

link_note() {
  local source="$1" target="$2"
  awk -F '\t' -v source="$source" -v target="$target" '
    $1 == source && $2 == target {
      print substr($0, length($1) + length($2) + 3)
      exit
    }
  '
}

SELF_AUDIT_LINKS=$( cd "$WORK/spec" && "$COVMAP_BIN" ls-links self_audit )
ACTUAL_REPAIR_NOTE=$(printf '%s\n' "$SELF_AUDIT_LINKS" \
  | link_note @audit-source-next @audit-target-next)
[[ "$ACTUAL_REPAIR_NOTE" == "$REPAIR_NOTE" ]] \
  || { echo "  FAIL: repair changed the link note"; exit 1; }
ACTUAL_SECOND_NOTE=$(printf '%s\n' "$SELF_AUDIT_LINKS" \
  | link_note @audit-source-next @audit-target-two)
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
