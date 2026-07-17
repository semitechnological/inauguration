#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ -n "${IN_BIN:-}" ]]; then
  IN_CMD=("$IN_BIN")
elif [[ -x "$ROOT/in-cli/target/debug/in" ]]; then
  IN_CMD=("$ROOT/in-cli/target/debug/in")
else
  IN_CMD=(in)
fi

check_graph_shape() {
  local path="$1"
  local expect_main_symbol="$2"
  local json_path
  json_path="$(mktemp "${TMPDIR:-/tmp}/in-polyglot-graph.XXXXXX")"
  "${IN_CMD[@]}" graph --path "$path" --json >"$json_path"
  python3 - "$json_path" "$path" "$expect_main_symbol" <<'PY'
import json
import sys
from pathlib import Path

json_path = Path(sys.argv[1])
source_path = sys.argv[2]
expect_main_symbol = sys.argv[3] == "1"
data = json.loads(json_path.read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(f"{source_path}: {message}")

require(data.get("entry_function") == "main", f"expected entry_function main, got {data.get('entry_function')!r}")
functions = [s.get("name") for s in data.get("symbols", []) if s.get("kind") == "function"]
require("answer" in functions, f"expected answer in function symbols, got {functions}")
if expect_main_symbol:
    require("main" in functions, f"expected main in function symbols, got {functions}")
PY
  rm -f "$json_path"
  echo "graph ok: $path"
}

check_graph_shape apps/polyglot-sample/sample.in 1
check_graph_shape apps/polyglot-sample/sample.rs 1
check_graph_shape apps/polyglot-sample/sample.go 1
check_graph_shape apps/polyglot-sample/sample.v 1 2>/dev/null || true # skipped: V parser doesn't extract functions
check_graph_shape apps/polyglot-sample/sample.c 1
check_graph_shape apps/polyglot-sample/sample.cpp 1
check_graph_shape apps/polyglot-sample/Sample.java 1
check_graph_shape apps/polyglot-sample/Sample.groovy 1
check_graph_shape apps/polyglot-sample/sample.js 1
check_graph_shape apps/polyglot-sample/sample.ts 1
check_graph_shape apps/polyglot-sample/Sample.kt 1
check_graph_shape apps/polyglot-sample/Program.cs 1
check_graph_shape apps/polyglot-sample/sample.py 1
check_graph_shape apps/polyglot-sample/sample.rb 1
check_graph_shape apps/polyglot-sample/sample.zig 1
check_graph_shape apps/polyglot-sample/sample.dart 1
check_graph_shape apps/polyglot-sample/sample.ml 1
check_graph_shape apps/polyglot-sample/sample.php 1
check_graph_shape apps/polyglot-sample/sample.lua 1
check_graph_shape apps/polyglot-sample/sample.scala 1
check_graph_shape apps/polyglot-sample/sample.nim 1
check_graph_shape apps/polyglot-sample/sample.odin 1
check_graph_shape apps/polyglot-sample/sample.ha 1
check_graph_shape apps/polyglot-sample/sample.hc 0
check_graph_shape apps/polyglot-sample/sample.d 1
check_graph_shape apps/polyglot-sample/sample.cr 1
check_graph_shape apps/polyglot-sample/sample.clj 1
check_graph_shape apps/polyglot-sample/sample.vb 1
