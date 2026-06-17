#!/usr/bin/env bash
# Instantiate a playground task into a fresh workspace.
# Usage: ./run_task.sh <task-name>
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

# Ensure the binary is built.
COVMAP_BIN="$ROOT/target/release/covmap"
if [[ ! -x "$COVMAP_BIN" ]]; then
  echo "==> building covmap (release)"
  ( cd "$ROOT" && cargo build --release >/dev/null )
fi

TASK="${1:-}"
if [[ -z "$TASK" ]]; then
  echo "Usage: $0 <task-name>" >&2
  echo "Available tasks:" >&2
  ( cd "$HERE" && ls -d T*/ 2>/dev/null | sed 's|/$||' | sed 's|^|  |' )
  exit 2
fi

TASK_DIR="$HERE/$TASK"
if [[ ! -d "$TASK_DIR" ]]; then
  echo "Unknown task: $TASK" >&2
  exit 2
fi

TS="$(date +%Y%m%d-%H%M%S)"
WORK="$HERE/_runs/$TASK/$TS"
mkdir -p "$WORK"
cp -R "$TASK_DIR/fixture/." "$WORK/"
cp "$TASK_DIR/TASK.md" "$WORK/TASK.md"
cp "$TASK_DIR/grade.sh" "$WORK/grade.sh" 2>/dev/null || true
chmod +x "$WORK/grade.sh" 2>/dev/null || true

export COVMAP_BIN TASK_DIR WORK
if [[ -x "$TASK_DIR/setup.sh" ]]; then
  ( cd "$WORK" && COVMAP_BIN="$COVMAP_BIN" "$TASK_DIR/setup.sh" )
fi

echo
echo "Task workspace ready."
echo "  workspace: $WORK"
echo "  prompt:    $TASK_DIR/TASK.md"
echo
echo "Hand TASK.md to an agent with cwd=$WORK."
echo "The covmap binary is at: $COVMAP_BIN"
