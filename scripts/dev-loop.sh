#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

mkdir -p "$ROOT/.brisk/hotreload"

exec cargo run --manifest-path in-cli/Cargo.toml -- dev
