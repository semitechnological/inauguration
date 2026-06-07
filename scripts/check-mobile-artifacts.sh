#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if command -v xcodebuild >/dev/null 2>&1; then
  echo "mobile-artifacts check: apple xcframework skeleton"
  (
    cd in-cli
    cargo test -q mobile::apple -- --nocapture
  )
else
  echo "mobile-artifacts: skip apple (xcodebuild unavailable)"
fi

if [[ "${IN_TEST_SKIP_ANDROID:-}" == "1" || "${IN_TEST_SKIP_ANDROID:-}" == "true" || "${IN_TEST_SKIP_ANDROID:-}" == "TRUE" ]]; then
  echo "mobile-artifacts: skip android (IN_TEST_SKIP_ANDROID set)"
else
  echo "mobile-artifacts check: android aar skeleton"
  (
    cd in-cli
    cargo test -q mobile::android -- --nocapture
  )
fi

echo "mobile-artifacts checks passed"