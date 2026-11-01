#!/usr/bin/env bash
# Start the shell (one instance, fixed port 47565).
set -euo pipefail
cd "$(dirname "$0")/.."

if pgrep -x dfo-shell >/dev/null; then
  echo "dfo-shell is already running (close it, or tools/stop.sh)"
  exit 1
fi

mkdir -p /tmp/opencode
nohup ./target/debug/dfo-shell "$@" > /tmp/opencode/dfo-rust.log 2>&1 &
echo $! > /tmp/opencode/dfo-rust.pid
sleep 1
echo "pid $(cat /tmp/opencode/dfo-rust.pid)"
echo "url $(cat /tmp/dfo-shell-url 2>/dev/null || echo '(none)')"
echo "log /tmp/opencode/dfo-rust.log"
