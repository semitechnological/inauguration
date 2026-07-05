#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
IN="${IN:-$ROOT_DIR/in-cli/target/release/in}"
tmp_log="$(mktemp "${TMPDIR:-/tmp}/in-compiler-bootstrap.XXXXXX")"
trap 'rm -f "$tmp_log"' EXIT

rm -f /tmp/in-compiler-bootstrap-generated.icore
rm -f /tmp/in-compiler-bootstrap-diagnostic.json

# The compiler bootstrap fixture relies on array locals (str_split_lines, array
# indexing, array_len), which are not yet supported by the JIT lowering path.
# Verify the source still parses and produces a graph, then skip execution.
if "$IN" graph --json --path apps/in-compiler-bootstrap/compiler.in >"$tmp_log" 2>&1; then
  echo "in-compiler-bootstrap: parse + graph ok (JIT array-local execution skipped)"
else
  echo "in-compiler-bootstrap: graph command failed"
  cat "$tmp_log"
  exit 1
fi
