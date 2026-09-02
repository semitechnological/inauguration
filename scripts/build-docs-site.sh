#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SITE="$ROOT/docs-site"
OUT="${OUT:-$SITE/dist}"
IN_BIN="${IN_BIN:-$(command -v in 2>/dev/null || true)}"

if [[ -n "$IN_BIN" && -f "$SITE/scripts/gen-splash.in" ]]; then
  "$IN_BIN" execute --path "$SITE/scripts/gen-splash.in" --module-id inauguration.docs.gen-splash 2>/dev/null || true
fi

if ! command -v bun >/dev/null 2>&1; then
  echo "error: bun is required to build the docs-site (moonshine static export)" >&2
  echo "  https://bun.sh" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is required to generate docs HTML via docs-site/docs-gen" >&2
  exit 1
fi

(cd "$SITE" && bun install --frozen-lockfile)
export DOCS_SITE_OUT="$OUT"
(cd "$SITE" && bun run src/build.ts)

"$ROOT/scripts/normalize-docs-site.sh" "$OUT"
echo "inauguration.tsc.hk" >"$OUT/CNAME"
