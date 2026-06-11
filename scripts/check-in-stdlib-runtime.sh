#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/in-stdlib-runtime.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT
command in execute-bytecode --verbose conformance/runtime/stdlib-path.in >"$tmp_dir/path.log" 2>&1
grep -q 'Execution completed with result: String("compiler.in")' "$tmp_dir/path.log"
command in execute-bytecode --verbose conformance/runtime/stdlib-env.in >"$tmp_dir/env.log" 2>&1
grep -q 'Execution completed with result: Bool(true)' "$tmp_dir/env.log"
command in execute-bytecode --verbose conformance/runtime/stdlib-fs.in >"$tmp_dir/fs.log" 2>&1
grep -q 'Execution completed with result: String("hello compiler")' "$tmp_dir/fs.log"
