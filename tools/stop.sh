#!/usr/bin/env bash
# Stop the shell (and its WebKit children).
set -euo pipefail
if [ -f /tmp/opencode/dfo-rust.pid ]; then
  pid=$(cat /tmp/opencode/dfo-rust.pid)
  if kill -0 "$pid" 2>/dev/null; then kill "$pid"; sleep 1; fi
  rm -f /tmp/opencode/dfo-rust.pid
fi
pkill -x dfo-shell 2>/dev/null || true
echo "stopped"
