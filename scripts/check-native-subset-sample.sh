#!/usr/bin/env bash
set -euo pipefail

# Swift is now a Tree-sitter Core IR front like all other languages.
# The `in build` command works directly without any env vars.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for sample in \
  apps/native-subset-sample/App.swift \
  apps/native-subset-sample/MultilineFields.swift \
  apps/native-subset-sample/SemicolonFields.swift
do
  "${IN_BIN:-in}" build \
    --path "$sample" \
    --module-id App
done
