#!/usr/bin/env bash
# Cover, refine spec/auth.md at its headings, NAME the Login slot,
# link it to the implementation, then edit Login's prose in place.
#
# The edit changes bytes inside the Login slot WITHOUT moving any line
# boundary. Because the connection references the slot by its name-tag
# (@login-contract), the pair is NOT broken by the byte change — the
# agent only needs to `recut` the slot to refresh its segment hash.
set -euo pipefail

( cd spec    && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null

# Refine spec/auth.md at lines 5, 10, 15 (the three `##` headings).
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'auth.md' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H"  5 "preamble" "Login" ) >/dev/null
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'auth.md:5' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H" 10 "Login" "Logout" ) >/dev/null
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'auth.md:10' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H" 15 "Logout" "Expiry" ) >/dev/null

# Name the Login slot and link it to the single impl segment.
LOGIN=$( cd spec && "$COVMAP_BIN" grep -l -c main "## Login" | head -1 )
( cd spec && "$COVMAP_BIN" name "main:$LOGIN" login-contract ) >/dev/null
IMPL_H=$( cd impl_rs && "$COVMAP_BIN" ls-segments )
( cd spec && "$COVMAP_BIN" connect main "$(pwd)/../impl_rs:main" auth_links ) >/dev/null
( cd spec && "$COVMAP_BIN" link auth_links @login-contract "$IMPL_H" "fn login" ) >/dev/null

# Drift: tighten the Login contract wording. Same number of lines, so
# the Login slot's boundaries are intact -- only its bytes change.
python3 - <<'PY'
from pathlib import Path
p = Path("spec/auth.md")
t = p.read_text()
t = t.replace(
    "On failure it returns an error after a fixed delay.",
    "On failure it returns an error after a fixed delay to deter timing attacks.",
)
p.write_text(t)
PY

echo "Setup done. The Login slot (@login-contract) is linked via auth_links"
echo "and its prose has been edited in place; refresh it with recut."
