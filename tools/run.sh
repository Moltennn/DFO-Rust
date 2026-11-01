#!/usr/bin/env bash
# Start the shell (one instance, fixed port).
set -euo pipefail
cd "$(dirname "$0")/.."

STATE="${DFO_STATE_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/dfo-rust}"
mkdir -p "$STATE"

PREFIX="${DFO_WEBKIT_PREFIX:-$HOME/.local/webkitgtk-patched}"
export LD_LIBRARY_PATH="$PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export PATH="$PREFIX/bin:$PATH"

# Mods by default when the workspace sits beside this repo; --mods/--no-mods/
# --url say otherwise.
MODS_DIR="${DFO_MODS_DIR:-../DFO-Mods}"
want_mods=1
for arg in "$@"; do
  case "$arg" in --mods|--no-mods|--url) want_mods=0 ;; esac
done
if [ "$want_mods" = 1 ] && [ -d "$MODS_DIR" ]; then
  set -- --mods "$(cd "$MODS_DIR" && pwd)" "$@"
fi

if pgrep -x dfo-shell >/dev/null; then
  echo "dfo-shell is already running (close it, or tools/stop.sh)"
  exit 1
fi

nohup ./target/debug/dfo-shell "$@" > "$STATE/shell.log" 2>&1 &
echo $! > "$STATE/shell.pid"
sleep 1
echo "pid $(cat "$STATE/shell.pid")"
echo "url $(cat "$STATE/url" 2>/dev/null || echo '(none)')"
echo "log $STATE/shell.log"
