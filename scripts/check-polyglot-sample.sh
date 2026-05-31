#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IN_CMD=("${IN_BIN:-in}")
POLYGLOT_OUT_DIR="target/in/polyglot"
mkdir -p "$POLYGLOT_OUT_DIR"

assert_empty_external_invocations() {
  local json_path="$1"
  python3 - "$json_path" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

require(
    data.get("external_invocations") == [],
    "compile external_invocations was not empty",
)
PY
}

assert_owned_compile_json() {
  local json_path="$1"
  assert_empty_external_invocations "$json_path"
  python3 - "$json_path" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

require(data.get("owned") is True, "compile report owned was not true")

success = data.get("success")
if success is True:
    require(
        data.get("executable_path") or data.get("artifact_path"),
        "successful compile missing executable_path or artifact_path",
    )
elif success is False:
    require(
        data.get("reason_code"),
        "failed compile missing reason_code",
    )
else:
    raise SystemExit("compile report missing success true/false")
PY
}

run_compile_owned() {
  local path="$1"
  local out_path="$POLYGLOT_OUT_DIR/$(basename "$path").out"
  local json_path
  json_path="$(mktemp "${TMPDIR:-/tmp}/in-polyglot-compile.XXXXXX")"
  "${IN_CMD[@]}" compile \
    --path "$path" \
    --target native \
    --entry answer \
    --out "$out_path" \
    --json >"$json_path"
  assert_owned_compile_json "$json_path"
  rm -f "$json_path"
  echo "polyglot ok: $path"
}

run_swift_subset_ok() {
  local path="$1"
  IN_NATIVE_SWIFT_SIL=only "${IN_CMD[@]}" build --path "$path" --module-id App
  run_compile_owned "$path"
}

run_icore_redirect() {
  local path="$1"
  echo "polyglot icore-redirect: $path"
  local output
  if output="$("${IN_CMD[@]}" build --path "$path" --module-id App 2>&1)"; then
    printf '%s\n' "$output"
    echo "expected $path to require .icore"
    return 1
  fi
  if [[ "$output" != *".icore"* ]]; then
    printf '%s\n' "$output"
    echo "expected $path failure to mention .icore"
    return 1
  fi
  local out_path="$POLYGLOT_OUT_DIR/$(basename "$path").out"
  local json_path
  json_path="$(mktemp "${TMPDIR:-/tmp}/in-polyglot-compile.XXXXXX")"
  "${IN_CMD[@]}" compile \
    --path "$path" \
    --target native \
    --entry answer \
    --out "$out_path" \
    --json >"$json_path"
  assert_empty_external_invocations "$json_path"
  rm -f "$json_path"
}

run_compile_owned apps/polyglot-sample/sample.in
run_compile_owned apps/polyglot-sample/sample.icore
run_swift_subset_ok apps/polyglot-sample/sample.swift
run_compile_owned apps/polyglot-sample/sample.rs
run_compile_owned apps/polyglot-sample/sample.go
run_compile_owned apps/polyglot-sample/sample.v
run_compile_owned apps/polyglot-sample/sample.c
run_compile_owned apps/polyglot-sample/sample.cpp
run_compile_owned apps/polyglot-sample/Sample.java
run_compile_owned apps/polyglot-sample/Sample.groovy
run_compile_owned apps/polyglot-sample/sample.js
run_compile_owned apps/polyglot-sample/sample.ts
run_compile_owned apps/polyglot-sample/Sample.kt
run_compile_owned apps/polyglot-sample/Program.cs
run_compile_owned apps/polyglot-sample/sample.py
run_compile_owned apps/polyglot-sample/sample.rb
run_compile_owned apps/polyglot-sample/sample.zig
run_compile_owned apps/polyglot-sample/sample.dart
run_compile_owned apps/polyglot-sample/sample.ml
run_compile_owned apps/polyglot-sample/sample.php
run_compile_owned apps/polyglot-sample/sample.lua
run_compile_owned apps/polyglot-sample/sample.scala
run_icore_redirect apps/polyglot-sample/sample.nim
run_icore_redirect apps/polyglot-sample/sample.odin
run_icore_redirect apps/polyglot-sample/sample.ha
