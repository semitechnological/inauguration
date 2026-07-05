#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="${CF_PAGES_PROJECT:-inauguration}"
# Prefer local crepuscularity checkout (void <br> fix) when global crepus is stale
export CREPU_ROOT="${CREPU_ROOT:-$ROOT/../crepuscularity}"
"$ROOT/scripts/build-docs-site.sh"
cd "$ROOT/docs-site"
wrangler pages deploy dist --project-name "$PROJECT" "$@"