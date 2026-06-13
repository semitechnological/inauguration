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
