#!/usr/bin/env bash
set -euo pipefail

# Hermetic icore JSON builds: Core IR + lower to SIL — no swiftc.
# Schema: `in-cli/src/compiler/icore.rs`, samples `apps/icore-sample/`.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo run --manifest-path in-cli/Cargo.toml --bin in -- build \
  --path apps/icore-sample/min-v1.icore \
  --module-id App \
  --parser auto

exec cargo run --manifest-path in-cli/Cargo.toml --bin in -- build \
  --path apps/icore-sample/min.icore \
  --module-id App \
  --parser auto
