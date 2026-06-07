#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ABI_HEADER="$ROOT/shared/abi/in_abi.h"
for symbol in in_arena_create in_arena_reset in_arena_destroy in_buf_from_host_arena in_borrow_bytes in_borrow_validate; do
  if ! grep -q "$symbol" "$ABI_HEADER"; then
    echo "dynamic-loader: missing symbol declaration $symbol in in_abi.h" >&2
    exit 1
  fi
done

echo "dynamic-loader check: arena runtime symbols"
(
  cd in-cli
  cargo test -q arena -- --nocapture
)
echo "dynamic-loader checks passed"