#!/usr/bin/env bash
# inauguration `in plugin run crepuscularity` — docs-site (.in) + crepuscularity-lite inauguration bridge
set -euo pipefail
TARGET="${1:-.}"
ROOT="${2:-$(pwd)}"
IN_BIN="${IN_BIN:-$(command -v in || echo "$HOME/.cargo/bin/in")}"

echo "[crepuscularity] native plugin target=$TARGET root=$ROOT"

if [[ -f "$ROOT/docs-site/build.in" ]]; then
  echo "[crepuscularity] docs-site build.in"
  (cd "$ROOT" && "$IN_BIN" execute --path docs-site/build.in --module-id inauguration.docs.build) || true
fi

if [[ -d "$TARGET" ]]; then
  find "$TARGET" -name '*.crepus' 2>/dev/null | while IFS= read -r file; do
    echo "[crepuscularity] compile $file"
    "$IN_BIN" build --path "$file" --module-id "$(basename "$file" .crepus)" || true
  done
fi

echo "[crepuscularity] plugin complete"
