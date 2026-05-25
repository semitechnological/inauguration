#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ "$(uname -s)" != "Darwin" ]] || [[ "$(uname -m)" != "arm64" ]]; then
  echo "native-answer-sample: skip (requires macOS aarch64)"
  exit 0
fi

IN_CMD=("${IN_BIN:-in}")
OUT="target/in/answer-sample"
JSON="$(mktemp "${TMPDIR:-/tmp}/in-native-answer.XXXXXX")"
trap 'rm -f "$JSON"' EXIT

mkdir -p target/in

"${IN_CMD[@]}" compile \
  --path apps/polyglot-sample/sample.in \
  --target native \
  --entry answer \
  --out "$OUT" \
  --json >"$JSON"

python3 - "$JSON" "$OUT" <<'PY'
import json
import subprocess
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())
binary = Path(sys.argv[2])

def require(condition, message):
    if not condition:
        raise SystemExit(message)

require(data.get("owned") is True, "compile report owned was not true")
require(
    data.get("external_invocations") == [],
    "compile external_invocations was not empty",
)
require(data.get("success") is True, f"native compile failed: {data.get('reason_code')} {data.get('reason')}")
require(binary.exists(), f"missing native executable: {binary}")

eval_exit = data.get("eval_exit_code")
if eval_exit == 42:
    print("native-answer-sample ok: eval_exit_code=42")
    sys.exit(0)

status = subprocess.run([str(binary)], check=False)
if status.returncode == 42:
    print("native-answer-sample ok: binary exited 42")
    sys.exit(0)

raise SystemExit(
    f"expected exit 42 (eval_exit_code={eval_exit!r}, binary={status.returncode})"
)
PY
