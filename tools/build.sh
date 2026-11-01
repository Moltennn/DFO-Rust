#!/usr/bin/env bash
# Build the shell against the private patched WebKitGTK 6.0.
set -euo pipefail
cd "$(dirname "$0")/.."
PREFIX="${DFO_WEBKIT_PREFIX:-$HOME/.local/webkitgtk-patched}"
export DFO_WEBKIT_PREFIX="$PREFIX"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
cargo build "$@"
echo "built ./target/debug/dfo-shell"
