#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IN_CMD=("${IN_BIN:-in}")
OUT="target/in/polyglot-sample.bca"

"${IN_CMD[@]}" compile-bytecode apps/polyglot-sample/sample.in --module-id App --out "$OUT"
grep -q 'call helper 1' "$OUT"
"${IN_CMD[@]}" run-bytecode "$OUT"
"${IN_CMD[@]}" execute-bytecode apps/polyglot-sample/sample.in --module-id App
"${IN_CMD[@]}" execute-bytecode apps/polyglot-sample/sample.ml --module-id App
output="$("${IN_CMD[@]}" execute-bytecode apps/in-sample/agent-native.in --module-id App --verbose 2>&1)"
printf '%s\n' "$output" | grep -qx 'ready'
