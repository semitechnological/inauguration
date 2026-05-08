#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
AURORALITY_ROOT="${AURORALITY_ROOT:-$ROOT/../aurorality}"
OUT_DIR="$ROOT/docs/benchmarks"
OUT_MD="$OUT_DIR/swift-vs-in.md"
OUT_JSON="$OUT_DIR/swift-vs-in.json"

mkdir -p "$OUT_DIR"

examples=(
  "$AURORALITY_ROOT/examples/counter/Sources/App.swift:Counter"
  "$AURORALITY_ROOT/examples/basic/Sources/App.swift:Basic"
  "$AURORALITY_ROOT/examples/hyperchat/Sources/HyperChatRootView.swift:HyperChat"
)

python3 - "$ROOT" "$OUT_JSON" "${examples[@]}" <<'PY'
import json
import subprocess
import sys
import time
from pathlib import Path

root = Path(sys.argv[1])
out_json = Path(sys.argv[2])
examples = sys.argv[3:]

def run(cmd, cwd):
    t0 = time.perf_counter()
    p = subprocess.run(cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    dt = (time.perf_counter() - t0) * 1000
    return p.returncode == 0, dt, p.stdout, p.stderr

def find_package_root(path: Path):
    cur = path.parent
    while cur != cur.parent:
        if (cur / "Package.swift").exists():
            return cur
        cur = cur.parent
    return None

rows = []
for entry in examples:
    path_str, module = entry.split(":", 1)
    path = Path(path_str)
    if not path.exists():
        rows.append({"example": path_str, "module": module, "error": "missing file"})
        continue

    pkg_root = find_package_root(path)

    ok_swiftc, ms_swiftc, _o1, e1 = run(["swiftc", "-typecheck", str(path)], root)

    if pkg_root:
        ok_swiftpkg, ms_swiftpkg, _o2, e2 = run(["swift", "build"], pkg_root)
    else:
        ok_swiftpkg, ms_swiftpkg, e2 = False, 0.0, "no Package.swift found"

    ok_in, ms_in, _o3, e3 = run(["in", "build", "--path", str(path), "--module-id", module], root)

    rows.append({
        "example": str(path),
        "module": module,
        "swiftc_ok": ok_swiftc,
        "swiftc_ms": round(ms_swiftc, 2),
        "swiftpkg_ok": ok_swiftpkg,
        "swiftpkg_ms": round(ms_swiftpkg, 2),
        "in_ok": ok_in,
        "in_ms": round(ms_in, 2),
        "speed_ratio_in_over_swiftpkg": round((ms_in / ms_swiftpkg), 3) if ms_swiftpkg > 0 else None,
        "swiftc_error": e1.strip()[:500],
        "swiftpkg_error": e2.strip()[:500],
        "in_error": e3.strip()[:500],
    })

out_json.write_text(json.dumps({"rows": rows}, indent=2))
PY

python3 - "$OUT_JSON" "$OUT_MD" <<'PY'
import json
import sys
from pathlib import Path

src = Path(sys.argv[1])
out = Path(sys.argv[2])
rows = json.loads(src.read_text())["rows"]

lines = []
lines.append("# Swift Compiler vs in Pipeline Benchmark")
lines.append("")
lines.append("Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.")
lines.append("")
lines.append("| Example | swiftc(ms) | swift build(ms) | in(ms) | in/swift-build | swift build ok | in ok |")
lines.append("|---|---:|---:|---:|---:|:---:|:---:|")
for row in rows:
    if "error" in row:
        lines.append(f"| `{row['example']}` | - | - | - | - | ❌ | ❌ |")
        continue
    lines.append(
        f"| `{row['example']}` | {row['swiftc_ms']} | {row['swiftpkg_ms']} | {row['in_ms']} | {row['speed_ratio_in_over_swiftpkg']} | {'✅' if row['swiftpkg_ok'] else '❌'} | {'✅' if row['in_ok'] else '❌'} |"
    )

out.write_text("\n".join(lines) + "\n")
PY

echo "wrote $OUT_MD"
echo "wrote $OUT_JSON"
