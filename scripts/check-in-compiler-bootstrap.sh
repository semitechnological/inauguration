#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
tmp_log="$(mktemp "${TMPDIR:-/tmp}/in-compiler-bootstrap.XXXXXX")"
trap 'rm -f "$tmp_log"' EXIT
rm -f /tmp/in-compiler-bootstrap-generated.icore
command in execute-bytecode --verbose apps/in-compiler-bootstrap/compiler.in >"$tmp_log" 2>&1
grep -q 'Execution completed with result: String("{\\"icoreVersion\\":2,\\"decls\\":' "$tmp_log"
grep -q '\\"name\\":\\"base\\"' "$tmp_log"
grep -q '\\"name\\":\\"ignored\\"' "$tmp_log"
grep -q '\\"name\\":\\"answer\\"' "$tmp_log"
grep -q '\\"kind\\":\\"binary\\"' "$tmp_log"
grep -q '\\"kind\\":\\"call\\"' "$tmp_log"
grep -q '\\"op\\":\\"*"' "$tmp_log"
test -s /tmp/in-compiler-bootstrap-generated.icore
command in build --path /tmp/in-compiler-bootstrap-generated.icore >/dev/null
command in execute-bytecode --verbose /tmp/in-compiler-bootstrap-generated.icore >"$tmp_log" 2>&1
grep -q 'Execution completed with result: Int(42)' "$tmp_log"
