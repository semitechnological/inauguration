#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SRC="apps/native-artifact-sample/answer.in"
OUT="target/in/native-windows-executable"
mkdir -p "$OUT"

env in compile --path "$SRC" --target native --target-triple x86_64-pc-windows-msvc --linkage executable --entry answer --out "$OUT/answer.exe" --json > "$OUT/windows.json"

python3 - "$OUT" <<'PY'
from pathlib import Path
import json
import sys

out = Path(sys.argv[1])
data = (out / "answer.exe").read_bytes()
report = json.loads((out / "windows.json").read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

require(report.get("success") is True, "compile failed")
require(report.get("runtime_level") == "windows-exitprocess", "runtime level mismatch")
require(report.get("reason_code") == "native-x86_64-windows-exe-subset", "reason mismatch")
require(data[:2] == b"MZ", "missing MZ magic")
pe_off = int.from_bytes(data[0x3C:0x40], "little")
require(data[pe_off:pe_off + 4] == b"PE\0\0", "missing PE signature")
require(int.from_bytes(data[pe_off + 4:pe_off + 6], "little") == 0x8664, "machine mismatch")
require(b"KERNEL32.dll" in data, "missing kernel32 import")
require(b"ExitProcess" in data, "missing ExitProcess import")
PY

run_and_check() {
  set +e
  "$@"
  code=$?
  set -e
  if [[ "$code" -ne 42 ]]; then
    echo "native-windows-executable: runtime exit mismatch: $code" >&2
    exit 1
  fi
}

if command -v wine64 >/dev/null 2>&1; then
  run_and_check wine64 "$OUT/answer.exe"
elif command -v wine >/dev/null 2>&1; then
  run_and_check wine "$OUT/answer.exe"
elif command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  set +e
  docker run --rm --platform linux/amd64 -v "$ROOT/$OUT:/work:ro" alpine:3.20 sh -c 'apk add --no-cache wine >/dev/null && wine /work/answer.exe'
  code=$?
  set -e
  if [[ "$code" -ne 42 ]]; then
    echo "native-windows-executable: Docker/Wine exit mismatch: $code" >&2
    exit 1
  fi
else
  echo "native-windows-executable: skip runtime (wine and Docker unavailable)"
fi

echo "native Windows executable checks passed"
