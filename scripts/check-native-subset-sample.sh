#!/usr/bin/env bash
set -euo pipefail

# Hermetic subset build: no swiftc (IN_NATIVE_SWIFT_SIL=only).
# See docs/architecture/subset-grammar.md

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export IN_NATIVE_SWIFT_SIL=only
exec "${IN_BIN:-in}" build \
  --path apps/native-subset-sample/App.swift \
  --module-id App
