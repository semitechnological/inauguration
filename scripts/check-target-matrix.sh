#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "target-matrix check: native host target contract"
(
  cd in-cli
  cargo test -q native_emit::lower::host_supports_native_subset -- --nocapture
)
python3 - <<'PY'
from pathlib import Path
root = Path(".")
triple = "aarch64-apple-darwin"
lower = (root / "in-cli/src/native_emit/lower.rs").read_text()
if "TARGET_TRIPLE" not in lower or triple not in lower:
    raise SystemExit("missing host target triple contract")
PY
echo "target-matrix checks passed"