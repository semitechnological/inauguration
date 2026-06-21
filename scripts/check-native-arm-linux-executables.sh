#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SRC="apps/native-artifact-sample/answer.in"
OUT="target/in/native-arm-linux-executables"
mkdir -p "$OUT"

env in compile --path "$SRC" --target native --target-triple aarch64-unknown-linux-gnu --linkage executable --entry answer --out "$OUT/answer-aarch64" --json > "$OUT/aarch64.json"
env in compile --path "$SRC" --target native --target-triple armv7-unknown-linux-gnueabihf --linkage executable --entry answer --out "$OUT/answer-armv7" --json > "$OUT/armv7.json"

python3 - "$OUT" <<'PY'
from pathlib import Path
import json
import sys

out = Path(sys.argv[1])

def report(name):
    return json.loads((out / name).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

aarch64 = (out / "answer-aarch64").read_bytes()
armv7 = (out / "answer-armv7").read_bytes()
require(report("aarch64.json").get("reason_code") == "native-aarch64-linux-exit-subset", "aarch64 reason mismatch")
require(aarch64[:4] == b"\x7fELF", "aarch64 missing ELF magic")
require(aarch64[4] == 2, "aarch64 class mismatch")
require(int.from_bytes(aarch64[16:18], "little") == 2, "aarch64 type mismatch")
require(int.from_bytes(aarch64[18:20], "little") == 183, "aarch64 machine mismatch")
require(report("armv7.json").get("reason_code") == "native-armv7-linux-exit-subset", "armv7 reason mismatch")
require(armv7[:4] == b"\x7fELF", "armv7 missing ELF magic")
require(armv7[4] == 1, "armv7 class mismatch")
require(int.from_bytes(armv7[16:18], "little") == 2, "armv7 type mismatch")
require(int.from_bytes(armv7[18:20], "little") == 40, "armv7 machine mismatch")
PY

if command -v qemu-aarch64 >/dev/null 2>&1; then
  chmod +x "$OUT/answer-aarch64"
  set +e
  qemu-aarch64 "$OUT/answer-aarch64"
  code=$?
  set -e
  if [[ "$code" -ne 42 ]]; then
    echo "qemu-aarch64 exit mismatch: $code" >&2
    exit 1
  fi
elif command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  if docker run --rm --platform linux/arm64 -v "$ROOT/$OUT:/work:ro" alpine:3.20 /bin/sh -c "exit 42" >/dev/null 2>&1; then
    set +e
    docker run --rm --platform linux/arm64 -v "$ROOT/$OUT:/work:ro" alpine:3.20 /work/answer-aarch64
    code=$?
    set -e
    if [[ "$code" -ne 42 ]]; then
      echo "docker linux/arm64 exit mismatch: $code" >&2
      exit 1
    fi
  else
    echo "native-arm-linux-executables: skip aarch64 runtime (Docker lacks linux/arm64 execution)"
  fi
else
  echo "native-arm-linux-executables: skip aarch64 runtime (qemu-aarch64 and Docker unavailable)"
fi

if command -v qemu-arm >/dev/null 2>&1; then
  chmod +x "$OUT/answer-armv7"
  set +e
  qemu-arm "$OUT/answer-armv7"
  code=$?
  set -e
  if [[ "$code" -ne 42 ]]; then
    echo "qemu-arm exit mismatch: $code" >&2
    exit 1
  fi
elif command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  if docker run --rm --platform linux/arm/v7 -v "$ROOT/$OUT:/work:ro" alpine:3.20 /bin/sh -c "exit 42" >/dev/null 2>&1; then
    set +e
    docker run --rm --platform linux/arm/v7 -v "$ROOT/$OUT:/work:ro" alpine:3.20 /work/answer-armv7
    code=$?
    set -e
    if [[ "$code" -ne 42 ]]; then
      echo "docker linux/arm/v7 exit mismatch: $code" >&2
      exit 1
    fi
  else
    echo "native-arm-linux-executables: skip armv7 runtime (Docker lacks linux/arm/v7 execution)"
  fi
else
  echo "native-arm-linux-executables: skip armv7 runtime (qemu-arm and Docker unavailable)"
fi

echo "native ARM Linux executable checks passed"
