#!/usr/bin/env bash
set -euo pipefail

# Hermetic icore JSON build: Core IR + lower to SIL — no swiftc.
# Schema: `in-cli/src/compiler/icore.rs`, sample `apps/icore-sample/min.icore`.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

exec cargo run --manifest-path in-cli/Cargo.toml --bin in -- build \
  --path apps/icore-sample/min.icore \
  --module-id App \
  --parser auto
