#!/usr/bin/env bash
# Cover, refine spec/policy.md at its headings, NAME + link the
# "Audit logs" section, then MOVE that section to the end of the file.
#
# Moving a segment is the case diff-remap cannot resolve on its own: the
# segment's boundary lines were deleted from their old position, so they
# fall in a changed region. `recut --remap` reports the slot as
# unresolved and prints a *fingerprint proposal* pointing at the segment's
# new location. The agent confirms the proposal and relinks. Nothing is
# moved automatically -- fingerprinting is propose-only.
set -euo pipefail

( cd spec    && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null

# Refine spec/policy.md at lines 5, 9, 13 (the three `##` headings).
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'policy.md' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H"  5 "preamble" "Audit" ) >/dev/null
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'policy.md:5' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H"  9 "Audit" "User" ) >/dev/null
H=$( cd spec && "$COVMAP_BIN" ls main | grep -F 'policy.md:9' | cut -f1 )
( cd spec && "$COVMAP_BIN" split "main:$H" 13 "User" "Backups" ) >/dev/null

# Name the Audit logs slot and link it to the implementation.
( cd spec && "$COVMAP_BIN" name "main:$( "$COVMAP_BIN" grep -l -c main '## Audit logs' | head -1 )" audit-retention ) >/dev/null
IMPL_H=$( cd impl_rs && "$COVMAP_BIN" ls-segments )
( cd spec && "$COVMAP_BIN" connect main "$(pwd)/../impl_rs:main" policy_links ) >/dev/null
( cd spec && "$COVMAP_BIN" link policy_links @audit-retention "$IMPL_H" "audit log retention" ) >/dev/null

# Drift by RELOCATION: move the Audit logs section to the end of the
# file. Diff-remap cannot place it; fingerprinting will propose it.
python3 - <<'PY'
from pathlib import Path
p = Path("spec/policy.md")
p.write_text(
    "# Retention Policy\n"
    "\n"
    "This document defines how long records are kept.\n"
    "\n"
    "## User data\n"
    "\n"
    "User data is kept until the account is closed and a grace period ends.\n"
    "\n"
    "## Backups\n"
    "\n"
    "Backups are encrypted at rest and rotated weekly.\n"
    "\n"
    "## Audit logs\n"
    "\n"
    "Audit logs are retained for ninety days, then purged automatically.\n"
)
PY

echo "Setup done. The '## Audit logs' section (@audit-retention) was moved"
echo "to the end of spec/policy.md. Run 'recut --remap' to see the proposal."
