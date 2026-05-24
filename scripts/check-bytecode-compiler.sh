#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IN_CMD=(cargo run --quiet --manifest-path in-cli/Cargo.toml --bin in --)
OUT="target/in/polyglot-sample.bca"

"${IN_CMD[@]}" compile-bytecode apps/polyglot-sample/sample.in --module-id App --out "$OUT"
"${IN_CMD[@]}" run-bytecode "$OUT"
"${IN_CMD[@]}" execute-bytecode apps/polyglot-sample/sample.in --module-id App
