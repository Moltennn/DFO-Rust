#!/usr/bin/env bash
# Stop the shell (and its WebKit children).
set -euo pipefail

STATE="${DFO_STATE_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/dfo-rust}"

if [ -f "$STATE/shell.pid" ]; then
  pid=$(cat "$STATE/shell.pid")
  if kill -0 "$pid" 2>/dev/null; then kill "$pid"; sleep 1; fi
  rm -f "$STATE/shell.pid"
fi
pkill -x dfo-shell 2>/dev/null || true
echo "stopped"
