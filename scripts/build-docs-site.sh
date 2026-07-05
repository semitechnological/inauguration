#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SITE="$ROOT/docs-site"
OUT="${OUT:-$SITE/dist}"
CREPUS_BIN="${CREPUS_BIN:-crepus}"

finish() {
  "$ROOT/scripts/patch-docs-site-instrument-sans.sh" "$OUT"
  echo "inauguration.tsc.hk" >"$OUT/CNAME"
}

if command -v "$CREPUS_BIN" >/dev/null 2>&1; then
  "$CREPUS_BIN" web build --site "$SITE" --out-dir "$OUT"
  finish
  exit 0
fi

CREPU_ROOT="${CREPU_ROOT:-$ROOT/../crepuscularity}"
if [[ ! -f "$CREPU_ROOT/Cargo.toml" ]]; then
  echo "error: crepus not on PATH and CREPU_ROOT=$CREPU_ROOT missing Cargo.toml" >&2
  echo "  Install: cargo install crepuscularity-cli" >&2
  echo "  Or set CREPU_ROOT to your crepuscularity checkout." >&2
  exit 1
fi

cargo run --manifest-path "$CREPU_ROOT/Cargo.toml" -p crepuscularity-cli -- \
  web build --site "$SITE" --out-dir "$OUT"

finish