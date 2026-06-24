#!/usr/bin/env bash
set -euo pipefail

# Swift is a Tree-sitter Core IR front behind '--features extended'.
# Skip if the `in` binary doesn't include the Swift parser.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

"${IN_BIN:-in}" compile --help 2>/dev/null | grep -q swift || {
  echo "[check-native-subset-sample] Swift parser not available (rebuild with --features extended); skipping"
  exit 0
}

for sample in \
  apps/native-subset-sample/App.swift \
  apps/native-subset-sample/MultilineFields.swift \
  apps/native-subset-sample/SemicolonFields.swift
do
  "${IN_BIN:-in}" build \
    --path "$sample" \
    --module-id App
done
