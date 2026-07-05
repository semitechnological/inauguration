#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="${CF_PAGES_PROJECT:-inauguration}"
"$ROOT/scripts/build-docs-site.sh"
cd "$ROOT/docs-site"
wrangler pages deploy dist --project-name "$PROJECT" "$@"