#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ "${IN_TEST_SKIP_ANDROID:-}" == "1" || "${IN_TEST_SKIP_ANDROID:-}" == "true" || "${IN_TEST_SKIP_ANDROID:-}" == "TRUE" ]]; then
  echo "mobile-artifacts: skip (IN_TEST_SKIP_ANDROID set)"
  exit 0
fi

echo "mobile-artifacts check: android aar skeleton"
(
  cd in-cli
  cargo test -q mobile::android -- --nocapture
)
echo "mobile-artifacts checks passed"