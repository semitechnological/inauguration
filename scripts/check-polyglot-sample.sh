#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IN_CMD=(cargo run --quiet --manifest-path in-cli/Cargo.toml --bin in --)

run_ok() {
  local path="$1"
  echo "polyglot ok: $path"
  "${IN_CMD[@]}" build --path "$path" --module-id App
}

run_swift_subset_ok() {
  local path="$1"
  echo "polyglot ok: $path"
  IN_NATIVE_SWIFT_SIL=only "${IN_CMD[@]}" build --path "$path" --module-id App
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
}

run_ok apps/polyglot-sample/sample.in
run_ok apps/polyglot-sample/sample.icore
run_swift_subset_ok apps/polyglot-sample/sample.swift
run_ok apps/polyglot-sample/sample.rs
run_ok apps/polyglot-sample/sample.go
run_ok apps/polyglot-sample/sample.v
run_ok apps/polyglot-sample/sample.c
run_ok apps/polyglot-sample/sample.cpp
run_ok apps/polyglot-sample/Sample.java
run_ok apps/polyglot-sample/Sample.groovy
run_ok apps/polyglot-sample/sample.js
run_ok apps/polyglot-sample/sample.ts
run_ok apps/polyglot-sample/Sample.kt
run_ok apps/polyglot-sample/Program.cs
run_ok apps/polyglot-sample/sample.py
run_ok apps/polyglot-sample/sample.rb
run_ok apps/polyglot-sample/sample.zig
run_ok apps/polyglot-sample/sample.dart
run_ok apps/polyglot-sample/sample.ml
run_icore_redirect apps/polyglot-sample/sample.nim
run_icore_redirect apps/polyglot-sample/sample.odin
run_icore_redirect apps/polyglot-sample/sample.ha
