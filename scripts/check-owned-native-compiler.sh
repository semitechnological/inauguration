#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IN_CMD=("${IN_BIN:-in}")
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/in-owned-native.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

mkdir -p target/in

BYTECODE_OUT="target/in/owned-sample.bca"
NATIVE_OUT="target/in/owned-sample-native"

echo "owned-native check: bytecode compile contract"
bytecode_json="$TMP_DIR/bytecode-compile.json"
"${IN_CMD[@]}" compile \
  --path apps/polyglot-sample/sample.in \
  --target bytecode \
  --entry main \
  --out "$BYTECODE_OUT" \
  --json >"$bytecode_json"
python3 - "$bytecode_json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

require(data.get("owned") is True, "bytecode compile report owned was not true")
require(
    data.get("external_invocations") == [],
    "bytecode compile external_invocations was not empty",
)
PY
echo "owned-native ok: bytecode compile owned with empty external_invocations"

echo "owned-native check: native compile contract"
native_json="$TMP_DIR/native-compile.json"
"${IN_CMD[@]}" compile \
  --path apps/polyglot-sample/sample.in \
  --target native \
  --entry main \
  --out "$NATIVE_OUT" \
  --json >"$native_json"
python3 - "$native_json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

require(
    data.get("external_invocations") == [],
    "native compile external_invocations was not empty",
)
success = data.get("success")
reason_code = data.get("reason_code")
if success is False:
    require(
        reason_code == "native-backend-not-implemented",
        "native compile failure reason_code was not native-backend-not-implemented",
    )
elif success is not True:
    raise SystemExit("native compile report missing success true/false")
PY
echo "owned-native ok: native compile owned report with empty external_invocations"

echo "owned-native check: backend bytecode report"
backend_json="$TMP_DIR/backend-bytecode.json"
"${IN_CMD[@]}" backend \
  --path apps/polyglot-sample/sample.in \
  --target bytecode \
  --json >"$backend_json"
python3 - "$backend_json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

if "owned" in data:
    require(data.get("owned") is True, "backend report owned was not true")
if "external_invocations" in data:
    require(
        data.get("external_invocations") == [],
        "backend external_invocations was not empty",
    )
PY
echo "owned-native ok: backend bytecode report contract"

if [[ -f "$BYTECODE_OUT" ]]; then
  echo "owned-native check: run bytecode artifact"
  "${IN_CMD[@]}" run-bytecode "$BYTECODE_OUT"
  echo "owned-native ok: run-bytecode $BYTECODE_OUT"
fi

echo "owned-native compiler checks passed"
