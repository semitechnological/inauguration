#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v xcodebuild >/dev/null 2>&1; then
  echo "mobile-artifacts: skip (xcodebuild unavailable)"
  exit 0
fi

echo "mobile-artifacts check: apple xcframework skeleton"
(
  cd in-cli
  cargo test -q mobile::apple -- --nocapture
)
echo "mobile-artifacts checks passed"