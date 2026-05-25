#!/usr/bin/env bash
set -euo pipefail

# Hermetic `.in` v0 build: in-tree parser + lower to SIL — no swiftc.
# Grammar: `in-cli/src/in_lang_parse.rs`.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

"${IN_BIN:-in}" build \
  --parser in \
  --path apps/in-sample/hello.in \
  --module-id App

"${IN_BIN:-in}" build \
  --parser in \
  --path apps/in-sample/agent-native.in \
  --module-id App
