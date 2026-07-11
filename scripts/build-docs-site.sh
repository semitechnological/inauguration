#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SITE="$ROOT/docs-site"
OUT="${OUT:-$SITE/dist}"
IN_BIN="${IN_BIN:-$(command -v in 2>/dev/null || true)}"

if [[ -n "$IN_BIN" && -f "$SITE/scripts/gen-splash.in" ]]; then
  "$IN_BIN" execute --path "$SITE/scripts/gen-splash.in" --module-id inauguration.docs.gen_splash 2>/dev/null || true
fi
CREPUS_BIN="${CREPUS_BIN:-crepus}"
CREPUS_MIN="0.9.18"
CREPU_ROOT="${CREPU_ROOT:-$ROOT/../crepuscularity}"

finish() {
  "$ROOT/scripts/normalize-docs-site.sh" "$OUT"
  echo "inauguration.tsc.hk" >"$OUT/CNAME"
}

crepus_ok() {
  command -v "$CREPUS_BIN" >/dev/null 2>&1 || return 1
  local ver
  ver="$("$CREPUS_BIN" --version 2>/dev/null | awk '{print $2}')"
  [[ -n "$ver" ]] && [[ "$(printf '%s\n' "$CREPUS_MIN" "$ver" | sort -V | head -1)" == "$CREPUS_MIN" ]]
}

if crepus_ok; then
  "$CREPUS_BIN" web build --site "$SITE" --out-dir "$OUT"
  finish
  exit 0
fi

if [[ -f "$CREPU_ROOT/Cargo.toml" ]]; then
  echo "note: using CREPU_ROOT=$CREPU_ROOT (PATH crepus missing or < $CREPUS_MIN)" >&2
  cargo run --manifest-path "$CREPU_ROOT/Cargo.toml" -p crepuscularity-cli -- \
    web build --site "$SITE" --out-dir "$OUT"
  finish
  exit 0
fi

echo "error: need crepuscularity-cli >= $CREPUS_MIN for correct <br> SSR" >&2
echo "  cargo install crepuscularity-cli --version $CREPUS_MIN --locked" >&2
echo "  or set CREPU_ROOT to a crepuscularity checkout with void-html fix" >&2
exit 1
