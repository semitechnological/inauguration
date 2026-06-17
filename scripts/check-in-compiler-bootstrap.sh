#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
IN="${IN:-$ROOT_DIR/in-cli/target/release/in}"
tmp_log="$(mktemp "${TMPDIR:-/tmp}/in-compiler-bootstrap.XXXXXX")"
trap 'rm -f "$tmp_log"' EXIT
rm -f /tmp/in-compiler-bootstrap-generated.icore
rm -f /tmp/in-compiler-bootstrap-diagnostic.json
"$IN" execute-bytecode --verbose apps/in-compiler-bootstrap/compiler.in >"$tmp_log" 2>&1
grep -q 'icoreVersion' "$tmp_log"
grep -q 'name.*base' "$tmp_log"
grep -q 'name.*ignored' "$tmp_log"
grep -q 'name.*answer' "$tmp_log"
grep -q 'kind.*binary' "$tmp_log"
grep -q 'kind.*call' "$tmp_log"
test -s /tmp/in-compiler-bootstrap-generated.icore
"$IN" build --path /tmp/in-compiler-bootstrap-generated.icore >/dev/null
"$IN" execute-bytecode --verbose /tmp/in-compiler-bootstrap-generated.icore >"$tmp_log" 2>&1
grep -q 'Int(42)' "$tmp_log"
tmp_sample="$(mktemp "${TMPDIR:-/tmp}/in-compiler-bootstrap-sample.XXXXXX")"
cp apps/in-compiler-bootstrap/sample.expr "$tmp_sample"
trap 'cp "$tmp_sample" apps/in-compiler-bootstrap/sample.expr; rm -f "$tmp_log" "$tmp_sample"' EXIT
printf 'let answer = nope\nanswer\n' > apps/in-compiler-bootstrap/sample.expr
rm -f /tmp/in-compiler-bootstrap-diagnostic.json
"$IN" execute-bytecode --verbose apps/in-compiler-bootstrap/compiler.in >"$tmp_log" 2>&1
grep -q '"code":"INBOOT001"' /tmp/in-compiler-bootstrap-diagnostic.json
grep -q '"reason":"unsupported-source"' /tmp/in-compiler-bootstrap-diagnostic.json
