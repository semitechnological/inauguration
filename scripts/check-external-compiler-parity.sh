#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IN_CMD=(cargo run --quiet --manifest-path in-cli/Cargo.toml --bin in --)
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/in-external-parity.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

run_in() {
  local path="$1"
  echo "in parity: $path"
  "${IN_CMD[@]}" build --path "$path" --module-id App >/dev/null
}

run_external() {
  local label="$1"
  shift
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "external skip: $label ($tool not found)"
    return 0
  fi
  echo "external ok: $label"
  "$@"
}

run_in apps/polyglot-sample/sample.c
run_external c cc -fsyntax-only apps/polyglot-sample/sample.c

run_in apps/polyglot-sample/sample.cpp
run_external c++ c++ -fsyntax-only apps/polyglot-sample/sample.cpp

run_in apps/polyglot-sample/sample.rs
run_external rust rustc apps/polyglot-sample/sample.rs -o "$TMP_DIR/sample-rs"

run_in apps/polyglot-sample/sample.go
if command -v go >/dev/null 2>&1; then
  echo "external ok: go"
  (cd apps/polyglot-sample && go run sample.go)
else
  echo "external skip: go (go not found)"
fi

run_in apps/polyglot-sample/sample.v
run_external v v -gc none -o "$TMP_DIR/sample-v" apps/polyglot-sample/sample.v

run_in apps/polyglot-sample/Sample.java
run_external java javac -d "$TMP_DIR/java" apps/polyglot-sample/Sample.java

run_in apps/polyglot-sample/sample.js
run_external javascript node --check apps/polyglot-sample/sample.js

run_in apps/polyglot-sample/sample.ts
run_external typescript tsc --noEmit --target es2020 apps/polyglot-sample/sample.ts

run_in apps/polyglot-sample/Sample.kt
run_external kotlin kotlinc apps/polyglot-sample/Sample.kt -d "$TMP_DIR/sample-kt.jar"

run_in apps/polyglot-sample/Program.cs
if command -v csc >/dev/null 2>&1; then
  echo "external ok: csharp"
  csc -nologo -out:"$TMP_DIR/Program.exe" apps/polyglot-sample/Program.cs
elif command -v mcs >/dev/null 2>&1; then
  echo "external ok: csharp"
  mcs -out:"$TMP_DIR/Program.exe" apps/polyglot-sample/Program.cs
else
  echo "external skip: csharp (csc or mcs not found)"
fi

run_in apps/polyglot-sample/sample.py
if command -v python3 >/dev/null 2>&1; then
  echo "external ok: python"
  PYTHONPYCACHEPREFIX="$TMP_DIR/pycache" python3 -m py_compile apps/polyglot-sample/sample.py
else
  echo "external skip: python (python3 not found)"
fi

run_in apps/polyglot-sample/sample.rb
run_external ruby ruby -c apps/polyglot-sample/sample.rb

run_in apps/polyglot-sample/sample.zig
run_external zig zig build-exe apps/polyglot-sample/sample.zig -femit-bin="$TMP_DIR/sample-zig"

run_in apps/polyglot-sample/sample.dart
run_external dart dart compile exe apps/polyglot-sample/sample.dart -o "$TMP_DIR/sample-dart"

run_in apps/polyglot-sample/sample.ml
run_external ocaml ocamlc -c apps/polyglot-sample/sample.ml -o "$TMP_DIR/sample.cmo"

echo "external compiler parity checks passed"
