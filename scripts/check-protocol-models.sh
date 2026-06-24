#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

"${PROTOCOL_GEN_BIN:-protocol-gen}" "$ROOT"

git diff --exit-code -- in-cli/src/hotreload/generated_protocol.rs
