#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:-.}"
ROOT="${2:-$(pwd)}"

echo "[aurorality] target=$TARGET"
# Fast lane for Aurorality-style Swift package projects.
if [[ -f "$TARGET/Package.swift" ]]; then
  (cd "$TARGET" && swift build -c release)
fi
# Warm inauguration pipeline cache on known source roots.
"$HOME/.cargo/bin/in" build --path "$TARGET/examples/hyperchat/Sources/HyperChatRootView.swift" --module-id HyperChat || true
"$HOME/.cargo/bin/in" build --path "$TARGET/examples/counter/Sources/App.swift" --module-id Counter || true
echo "[aurorality] plugin complete"
