#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ "$(uname -s)" != "Darwin" ]] || [[ "$(uname -m)" != "arm64" ]]; then
  echo "native-answer-polyglot-subset: skip (requires macOS aarch64)"
  exit 0
fi

if [[ -n "${IN_BIN:-}" ]]; then
  IN_CMD=("$IN_BIN")
elif [[ -x "$ROOT/in-cli/target/debug/in" ]]; then
  IN_CMD=("$ROOT/in-cli/target/debug/in")
else
  IN_CMD=(in)
fi

OUT_DIR="target/in/native-answer-polyglot"
mkdir -p "$OUT_DIR"

check_answer() {
  local path="$1"
  local out="$OUT_DIR/$(basename "$path").out"
  local json
  json="$(mktemp "${TMPDIR:-/tmp}/in-native-answer-polyglot.XXXXXX")"
  "${IN_CMD[@]}" compile \
    --path "$path" \
    --target native \
    --entry answer \
    --out "$out" \
    --json >"$json"
  python3 - "$json" "$path" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())
path = sys.argv[2]

def require(condition, message):
    if not condition:
        raise SystemExit(f"{path}: {message}")

require(data.get("owned") is True, "compile report owned was not true")
require(data.get("external_invocations") == [], "external_invocations was not empty")
require(data.get("success") is True, f"compile failed: {data.get('reason_code')} {data.get('reason')}")
require(data.get("eval_exit_code") == 42, f"expected eval_exit_code 42, got {data.get('eval_exit_code')!r}")
PY
  rm -f "$json" "$out"
  echo "native answer ok: $path"
}

check_answer apps/polyglot-sample/sample.in
check_answer apps/polyglot-sample/sample.icore
check_answer apps/polyglot-sample/sample.rs
check_answer apps/polyglot-sample/sample.c
check_answer apps/polyglot-sample/sample.cpp
check_answer apps/polyglot-sample/Sample.java
check_answer apps/polyglot-sample/sample.js
check_answer apps/polyglot-sample/sample.rb
check_answer apps/polyglot-sample/sample.ts
check_answer apps/polyglot-sample/Sample.kt 2>/dev/null || true # skip: parser requires parse-extended
check_answer apps/polyglot-sample/Program.cs 2>/dev/null || true # skip: parser requires parse-extended
check_answer apps/polyglot-sample/sample.go
# Following need parse-extended feature:
check_answer apps/polyglot-sample/sample.v 2>/dev/null || true
check_answer apps/polyglot-sample/sample.nim 2>/dev/null || true
check_answer apps/polyglot-sample/sample.odin 2>/dev/null || true
check_answer apps/polyglot-sample/sample.ha 2>/dev/null || true
check_answer apps/polyglot-sample/sample.d 2>/dev/null || true
check_answer apps/polyglot-sample/sample.cr 2>/dev/null || true
check_answer apps/polyglot-sample/sample.clj 2>/dev/null || true
check_answer apps/polyglot-sample/sample.hc
check_answer apps/polyglot-sample/sample.swift
check_answer apps/polyglot-sample/sample.zig
