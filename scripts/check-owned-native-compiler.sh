#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IN_CMD=("${IN_BIN:-in}")
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/in-owned-native.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

mkdir -p target/in

is_macos_aarch64() {
  [[ "$(uname -s)" == "Darwin" ]] && [[ "$(uname -m)" == "arm64" ]]
}

JIT_OUT="target/in/owned-sample-jit"
NATIVE_OUT="target/in/owned-sample-native"

echo "owned-native check: JIT compile contract"
jit_json="$TMP_DIR/jit-compile.json"
"${IN_CMD[@]}" compile \
  --path apps/polyglot-sample/sample.in \
  --target jit \
  --entry main \
  --out "$JIT_OUT" \
  --json >"$jit_json"
python3 - "$jit_json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

require(data.get("owned") is True, "JIT compile report owned was not true")
require(
    data.get("external_invocations") == [],
    "JIT compile external_invocations was not empty",
)
PY
echo "owned-native ok: JIT compile owned with empty external_invocations"

echo "owned-native check: native compile contract"
native_json="$TMP_DIR/native-compile.json"
"${IN_CMD[@]}" compile \
  --path apps/polyglot-sample/sample.in \
  --target native \
  --entry main \
  --out "$NATIVE_OUT" \
  --json >"$native_json"
if is_macos_aarch64; then
  MACOS_AARCH64_FLAG=yes
else
  MACOS_AARCH64_FLAG=no
fi
python3 - "$native_json" "$NATIVE_OUT" "$MACOS_AARCH64_FLAG" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())
native_out = Path(sys.argv[2])
macos_aarch64 = sys.argv[3] == "yes"

def require(condition, message):
    if not condition:
        raise SystemExit(message)

require(
    data.get("external_invocations") == [],
    "native compile external_invocations was not empty",
)
success = data.get("success")
reason_code = data.get("reason_code")
if macos_aarch64:
    require(success is True, f"native compile expected success on macOS aarch64: {reason_code} {data.get('reason')}")
    require(native_out.exists(), f"missing native executable: {native_out}")
else:
    if success is False:
        require(
            reason_code in {"native-backend-not-implemented", "native-host-unsupported"},
            "native compile failure reason_code was unexpected",
        )
    elif success is not True:
        raise SystemExit("native compile report missing success true/false")
PY
echo "owned-native ok: native compile owned report with empty external_invocations"

echo "owned-native check: backend native report"
backend_json="$TMP_DIR/backend-native.json"
"${IN_CMD[@]}" backend \
  --path apps/polyglot-sample/sample.in \
  --target native \
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
echo "owned-native ok: backend native report contract"
echo "owned-native compiler checks passed"
