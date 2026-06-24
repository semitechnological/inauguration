#!/usr/bin/env bash
# ponytail: crepuscularity native plugin — warm compilation cache + compile .crepus templates
set -euo pipefail
TARGET="${1:-.}"
ROOT="${2:-$(pwd)}"

echo "[crepuscularity] native plugin target=$TARGET"

# Compile .crepus templates through inauguration pipeline
if [[ -d "$TARGET" ]]; then
  find "$TARGET" -name '*.crepus' 2>/dev/null | while IFS= read -r file; do
    echo "[crepuscularity] compile $file"
    "$HOME/.cargo/bin/in" build --path "$file" --module-id "$(basename "$file" .crepus)" || true
  done
fi

# Prime hybrid compiler pipeline on swift files for cache warmth
if [[ -d "$TARGET" ]]; then
  find "$TARGET" -name '*.swift' 2>/dev/null | head -5 | while IFS= read -r file; do
    module="$(basename "$file" .swift)"
    "$HOME/.cargo/bin/in" build --path "$file" --module-id "$module" || true
  done
fi

echo "[crepuscularity] plugin complete"
