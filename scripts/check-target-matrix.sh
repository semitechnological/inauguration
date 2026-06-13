#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "target-matrix check: native host target contract"
(
  cd in-cli
  cargo test -q native_emit::lower::host_supports_native_subset -- --nocapture
  cargo test -q target::tests::registry_lists_rust_style_compiler_targets
)
python3 - <<'PY'
from pathlib import Path
root = Path(".")
triple = "aarch64-apple-darwin"
lower = (root / "in-cli/src/native_emit/lower.rs").read_text()
if "TARGET_TRIPLE" not in lower or triple not in lower:
    raise SystemExit("missing host target triple contract")
target = (root / "in-cli/src/target.rs").read_text()
for triple in [
    "x86_64-unknown-none",
    "x86_64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "armv7-unknown-linux-gnueabihf",
    "wasm32-unknown-unknown",
    "riscv64gc-unknown-none-elf",
]:
    if triple not in target:
        raise SystemExit(f"missing in target equivalent: {triple}")
PY
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
cat > "$tmpdir/object.in" <<'SRC'
fn answer() -> Int { return 42; }
fn main() -> void { return; }
SRC
env in compile \
  --path "$tmpdir/object.in" \
  --target native \
  --target-triple x86_64-unknown-linux-gnu \
  --linkage static-lib \
  --entry answer \
  --out "$tmpdir/object.o" \
  --json > "$tmpdir/object.json"
python3 - "$tmpdir/object.o" "$tmpdir/object.json" <<'PY'
from pathlib import Path
import json
import sys
data = Path(sys.argv[1]).read_bytes()
report = json.loads(Path(sys.argv[2]).read_text())
if data[:4] != b"\x7fELF":
    raise SystemExit("x86_64 object missing ELF magic")
if int.from_bytes(data[16:18], "little") != 1:
    raise SystemExit("x86_64 object is not ET_REL")
if int.from_bytes(data[18:20], "little") != 62:
    raise SystemExit("x86_64 object is not EM_X86_64")
for needle in [b".text", b".symtab", b".strtab", b".shstrtab", b"answer"]:
    if needle not in data:
        raise SystemExit(f"x86_64 object missing {needle!r}")
if report.get("reason_code") != "native-object-subset":
    raise SystemExit("x86_64 object report has wrong reason_code")
PY
env in compile \
  --path "$tmpdir/object.in" \
  --target native \
  --target-triple aarch64-unknown-linux-gnu \
  --linkage static-lib \
  --entry answer \
  --out "$tmpdir/aarch64.o" \
  --json > "$tmpdir/aarch64.json"
python3 - "$tmpdir/aarch64.o" "$tmpdir/aarch64.json" <<'PY'
from pathlib import Path
import json
import sys
data = Path(sys.argv[1]).read_bytes()
report = json.loads(Path(sys.argv[2]).read_text())
if data[:4] != b"\x7fELF":
    raise SystemExit("aarch64 object missing ELF magic")
if data[4] != 2:
    raise SystemExit("aarch64 object is not ELFCLASS64")
if int.from_bytes(data[16:18], "little") != 1:
    raise SystemExit("aarch64 object is not ET_REL")
if int.from_bytes(data[18:20], "little") != 183:
    raise SystemExit("aarch64 object is not EM_AARCH64")
for needle in [b".text", b".symtab", b".strtab", b".shstrtab", b"answer"]:
    if needle not in data:
        raise SystemExit(f"aarch64 object missing {needle!r}")
if report.get("reason_code") != "native-object-subset":
    raise SystemExit("aarch64 object report has wrong reason_code")
PY
env in compile \
  --path "$tmpdir/object.in" \
  --target native \
  --target-triple armv7-unknown-linux-gnueabihf \
  --linkage static-lib \
  --entry answer \
  --out "$tmpdir/arm32.o" \
  --json > "$tmpdir/arm32.json"
python3 - "$tmpdir/arm32.o" "$tmpdir/arm32.json" <<'PY'
from pathlib import Path
import json
import sys
data = Path(sys.argv[1]).read_bytes()
report = json.loads(Path(sys.argv[2]).read_text())
if data[:4] != b"\x7fELF":
    raise SystemExit("arm32 object missing ELF magic")
if data[4] != 1:
    raise SystemExit("arm32 object is not ELFCLASS32")
if int.from_bytes(data[16:18], "little") != 1:
    raise SystemExit("arm32 object is not ET_REL")
if int.from_bytes(data[18:20], "little") != 40:
    raise SystemExit("arm32 object is not EM_ARM")
for needle in [b".text", b".symtab", b".strtab", b".shstrtab", b"answer"]:
    if needle not in data:
        raise SystemExit(f"arm32 object missing {needle!r}")
if report.get("reason_code") != "native-object-subset":
    raise SystemExit("arm32 object report has wrong reason_code")
PY
env in compile \
  --path "$tmpdir/object.in" \
  --target native \
  --target-triple aarch64-apple-darwin \
  --linkage static-lib \
  --entry answer \
  --out "$tmpdir/libanswer.a" \
  --json > "$tmpdir/macho.json"
python3 - "$tmpdir/libanswer.a" "$tmpdir/macho.json" <<'PY'
from pathlib import Path
import json
import sys
data = Path(sys.argv[1]).read_bytes()
report = json.loads(Path(sys.argv[2]).read_text())
if data[:8] != b"!<arch>\n":
    raise SystemExit("mach-o staticlib missing ar magic")
offset = 82
if int.from_bytes(data[offset:offset + 4], "little") != 0xFEEDFACF:
    raise SystemExit("mach-o staticlib missing Mach-O magic")
if int.from_bytes(data[offset + 4:offset + 8], "little", signed=True) != 0x0100000C:
    raise SystemExit("mach-o staticlib missing ARM64 CPU type")
if int.from_bytes(data[offset + 12:offset + 16], "little") != 1:
    raise SystemExit("mach-o staticlib member is not MH_OBJECT")
if b"_answer" not in data:
    raise SystemExit("mach-o staticlib missing _answer export")
if report.get("reason_code") != "native-object-subset":
    raise SystemExit("mach-o staticlib report has wrong reason_code")
PY
env in compile \
  --path "$tmpdir/object.in" \
  --target native \
  --target-triple x86_64-unknown-linux-gnu \
  --linkage executable \
  --entry answer \
  --out "$tmpdir/x86-exe" \
  --json > "$tmpdir/x86-exe.json"
python3 - "$tmpdir/x86-exe" "$tmpdir/x86-exe.json" <<'PY'
from pathlib import Path
import json
import sys
data = Path(sys.argv[1]).read_bytes()
report = json.loads(Path(sys.argv[2]).read_text())
if data[:4] != b"\x7fELF":
    raise SystemExit("x86_64 executable missing ELF magic")
if int.from_bytes(data[16:18], "little") != 2:
    raise SystemExit("x86_64 executable is not ET_EXEC")
if int.from_bytes(data[18:20], "little") != 62:
    raise SystemExit("x86_64 executable is not EM_X86_64")
if bytes([0x48, 0xC7, 0xC0, 0x3C, 0, 0, 0, 0x48, 0xC7, 0xC7, 0x2A, 0, 0, 0, 0x0F, 0x05]) not in data:
    raise SystemExit("x86_64 executable missing syscall exit stub")
if report.get("reason_code") != "native-x86_64-linux-exit-subset":
    raise SystemExit("x86_64 executable report has wrong reason_code")
PY
env in compile \
  --path "$tmpdir/object.in" \
  --target native \
  --target-triple wasm32-unknown-unknown \
  --linkage static-lib \
  --entry answer \
  --out "$tmpdir/object.wasm" \
  --json > "$tmpdir/wasm.json"
python3 - "$tmpdir/object.wasm" "$tmpdir/wasm.json" <<'PY'
from pathlib import Path
import json
import sys
data = Path(sys.argv[1]).read_bytes()
report = json.loads(Path(sys.argv[2]).read_text())
if data[:4] != b"\0asm":
    raise SystemExit("wasm32 module missing wasm magic")
if b"answer" not in data:
    raise SystemExit("wasm32 module missing answer export")
if report.get("reason_code") != "native-object-subset":
    raise SystemExit("wasm32 module report has wrong reason_code")
PY
echo "target-matrix checks passed"
