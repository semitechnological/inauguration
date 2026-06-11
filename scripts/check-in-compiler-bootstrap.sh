#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
tmp_log="$(mktemp "${TMPDIR:-/tmp}/in-compiler-bootstrap.XXXXXX")"
trap 'rm -f "$tmp_log"' EXIT
command in execute-bytecode --verbose apps/in-compiler-bootstrap/compiler.in >"$tmp_log" 2>&1
grep -q 'Execution completed with result: String("let answer = 40 + 2\\nanswer\\n")' "$tmp_log"
