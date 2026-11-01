#!/usr/bin/env bash
# Run the GTK4 native shell against the private patched WebKitGTK 6.0.
#   ./tools/run-native.sh [shell args]
# Same state dir as tools/run.sh (url, logs, pid); the game store is shared, so
# never run both shells at once.
set -euo pipefail
cd "$(dirname "$0")/.."

STATE="${DFO_STATE_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/dfo-rust}"
mkdir -p "$STATE"

PREFIX="${DFO_WEBKIT_PREFIX:-$HOME/.local/webkitgtk-patched}"
export LD_LIBRARY_PATH="$PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export PATH="$PREFIX/bin:$PATH"

if pgrep -x dfo-shell-gtk4 >/dev/null; then
  echo "dfo-shell-gtk4 is already running (close it, or tools/stop.sh)"
  exit 1
fi
if pgrep -x dfo-shell >/dev/null; then
  echo "the wry shell is running (tools/stop.sh first: one store, one server)"
  exit 1
fi

nohup ./target/debug/dfo-shell-gtk4 "$@" > "$STATE/shell-native.log" 2>&1 &
echo $! > "$STATE/shell-native.pid"
sleep 1
echo "pid $(cat "$STATE/shell-native.pid")"
echo "url $(cat "$STATE/url" 2>/dev/null || echo '(none)')"
echo "log $STATE/shell-native.log"
