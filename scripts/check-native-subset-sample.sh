#!/usr/bin/env bash
set -euo pipefail

# Hermetic subset build: no swiftc (IN_NATIVE_SWIFT_SIL=only).
# See docs/architecture/subset-grammar.md

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export IN_NATIVE_SWIFT_SIL=only
for sample in \
  apps/native-subset-sample/App.swift \
  apps/native-subset-sample/MultilineFields.swift \
  apps/native-subset-sample/SemicolonFields.swift
do
  "${IN_BIN:-in}" build \
    --path "$sample" \
    --module-id App
done
