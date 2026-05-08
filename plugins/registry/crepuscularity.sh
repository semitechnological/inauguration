#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:-.}"
ROOT="${2:-$(pwd)}"

echo "[crepuscularity] target=$TARGET"
# Aggressive source discovery using ripgrep for hot files.
if [[ -d "$TARGET" ]]; then
  rg --files "$TARGET" -g '*.swift' > /tmp/crepuscularity-swift-files.txt || true
fi
# Prime hybrid compiler pipeline on first 5 swift files for lower first-hit latency.
count=0
while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  module="$(basename "$file" .swift)"
  "$HOME/.cargo/bin/in" build --path "$file" --module-id "$module" || true
  count=$((count+1))
  [[ $count -ge 5 ]] && break
done < /tmp/crepuscularity-swift-files.txt

echo "[crepuscularity] plugin complete"
