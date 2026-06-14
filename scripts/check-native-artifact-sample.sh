#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SRC="apps/native-artifact-sample/answer.in"
OUT="target/in/native-artifact-sample"
mkdir -p "$OUT"
rm -rf "$OUT/Answer.app" "$OUT/Answer.AppDir"

env in compile --path "$SRC" --target native --target-triple x86_64-unknown-linux-gnu --linkage executable --entry answer --out "$OUT/answer-linux-x86_64" --json > "$OUT/linux-exe.json"
env in compile --path "$SRC" --target native --target-triple aarch64-unknown-linux-gnu --linkage executable --entry answer --out "$OUT/answer-linux-aarch64" --json > "$OUT/aarch64-linux-exe.json"
env in compile --path "$SRC" --target native --target-triple armv7-unknown-linux-gnueabihf --linkage executable --entry answer --out "$OUT/answer-linux-armv7" --json > "$OUT/armv7-linux-exe.json"
env in compile --path "$SRC" --target native --target-triple x86_64-unknown-linux-gnu --linkage executable --entry answer --out "$OUT/Answer.AppDir" --json > "$OUT/appdir.json"
env in compile --path "$SRC" --target native --target-triple x86_64-pc-windows-msvc --linkage executable --entry answer --out "$OUT/answer.exe" --json > "$OUT/windows-exe.json"
env in compile --path "$SRC" --target native --target-triple aarch64-apple-darwin --linkage executable --entry answer --out "$OUT/Answer.app" --json > "$OUT/app.json"
env in compile --path "$SRC" --target native --target-triple x86_64-unknown-linux-gnu --linkage static-lib --entry answer --out "$OUT/answer-x86_64.o" --json > "$OUT/x86_64-object.json"
env in compile --path "$SRC" --target native --target-triple aarch64-unknown-linux-gnu --linkage static-lib --entry answer --out "$OUT/answer-aarch64.o" --json > "$OUT/aarch64-object.json"
env in compile --path "$SRC" --target native --target-triple armv7-unknown-linux-gnueabihf --linkage static-lib --entry answer --out "$OUT/answer-armv7.o" --json > "$OUT/armv7-object.json"
env in compile --path "$SRC" --target native --target-triple aarch64-apple-darwin --linkage static-lib --entry answer --out "$OUT/libanswer.a" --json > "$OUT/macho-staticlib.json"
env in compile --path "$SRC" --target native --target-triple wasm32-unknown-unknown --linkage static-lib --entry answer --out "$OUT/answer.wasm" --json > "$OUT/wasm.json"
env in compile --path "$SRC" --target native --target-triple x86_64-unknown-linux-gnu --linkage executable --entry answer --out "$OUT/Answer.AppImage" --json > "$OUT/appimage.json"

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

require(report("linux-exe.json").get("reason_code") == "native-x86_64-linux-exit-subset", "linux exe reason mismatch")
require((out / "answer-linux-x86_64").read_bytes()[:4] == b"\x7fELF", "linux exe missing ELF magic")
require(report("aarch64-linux-exe.json").get("reason_code") == "native-aarch64-linux-exit-subset", "aarch64 linux exe reason mismatch")
aarch64_exe = (out / "answer-linux-aarch64").read_bytes()
require(aarch64_exe[:4] == b"\x7fELF", "aarch64 linux exe missing ELF magic")
require(aarch64_exe[4] == 2, "aarch64 linux exe class mismatch")
require(int.from_bytes(aarch64_exe[18:20], "little") == 183, "aarch64 linux exe machine mismatch")
require(report("armv7-linux-exe.json").get("reason_code") == "native-armv7-linux-exit-subset", "armv7 linux exe reason mismatch")
armv7_exe = (out / "answer-linux-armv7").read_bytes()
require(armv7_exe[:4] == b"\x7fELF", "armv7 linux exe missing ELF magic")
require(armv7_exe[4] == 1, "armv7 linux exe class mismatch")
require(int.from_bytes(armv7_exe[18:20], "little") == 40, "armv7 linux exe machine mismatch")
require(report("appdir.json").get("reason_code") == "native-x86_64-linux-appdir-subset", "AppDir reason mismatch")
require((out / "Answer.AppDir" / "AppRun").read_bytes()[:4] == b"\x7fELF", "AppDir AppRun missing ELF magic")
require(report("windows-exe.json").get("reason_code") == "native-x86_64-windows-exe-subset", "windows exe reason mismatch")
require((out / "answer.exe").read_bytes()[:2] == b"MZ", "windows exe missing MZ magic")
require(report("app.json").get("reason_code") == "native-aarch64-darwin-app-subset", "app bundle reason mismatch")
require((out / "Answer.app" / "Contents" / "MacOS" / "Answer").read_bytes()[:4] == (0xFEEDFACF).to_bytes(4, "little"), "app executable missing Mach-O magic")
require(report("x86_64-object.json").get("reason_code") == "native-object-subset", "x86_64 object reason mismatch")
require(report("aarch64-object.json").get("reason_code") == "native-object-subset", "aarch64 object reason mismatch")
require(report("armv7-object.json").get("reason_code") == "native-object-subset", "armv7 object reason mismatch")
require(report("macho-staticlib.json").get("reason_code") == "native-object-subset", "mach-o staticlib reason mismatch")
require(report("wasm.json").get("reason_code") == "native-object-subset", "wasm reason mismatch")
require(report("appimage.json").get("success") is False, "AppImage should fail closed")
require(report("appimage.json").get("reason_code") == "native-package-not-implemented", "AppImage reason mismatch")
print("native artifact sample checks passed")
PY
